use super::models::{
    FlowStepType, LocalCondition, LocalFlow, LocalFlowStep, LocalFunctionStep,
    LocalTransitionFunction, RemoteCondition, RemoteFlow, RemoteFlowStep, RemoteFunctionStep,
    RemoteTransitionFunction,
};
use crate::functions::{
    function_create_latency_control, infer_function_description, local_latency_control_from_code,
    try_function_code_from_local_content,
};
use crate::{
    CommandGenError, FlowImportPathMaps, PromptReferenceMaps, replace_flow_import_names_with_ids,
    replace_resource_names_with_ids, yaml_str,
};
use adk_protobuf::flows::{StepAsrConfig, StepDtmfConfig, StepPosition};
use adk_protobuf::functions::FunctionCreateLatencyControl;
use adk_types::{Resource, ResourceMap};
use serde_json::Value;
use std::collections::HashMap;

pub(super) fn local_flows(
    resources: &ResourceMap,
    prompt_reference_maps: &PromptReferenceMaps,
    flow_import_path_maps: &FlowImportPathMaps,
) -> Result<Vec<LocalFlow>, CommandGenError> {
    let mut flows: HashMap<String, LocalFlow> = HashMap::new();

    for resource in resources.values() {
        let path = resource.file_path.as_str();
        if path.starts_with("flows/") && path.ends_with("/flow_config.yaml") {
            let Some(folder) = flow_folder_from_path(path) else {
                continue;
            };
            let Some(yaml) = resource_yaml(resource) else {
                continue;
            };
            let entry = flows.entry(folder.clone()).or_insert_with(|| LocalFlow {
                folder,
                config_path: path.to_string(),
                ..LocalFlow::default()
            });
            entry.config_path = path.to_string();
            entry.name = non_empty(yaml_str(&yaml, "name"), &entry.folder);
            entry.description = yaml_str(&yaml, "description");
            entry.start_step = yaml_str(&yaml, "start_step");
        } else if path.starts_with("flows/") && path.contains("/steps/") && path.ends_with(".yaml")
        {
            let Some(folder) = flow_folder_from_path(path) else {
                continue;
            };
            let Some(yaml) = resource_yaml(resource) else {
                continue;
            };
            let entry = flows.entry(folder.clone()).or_insert_with(|| LocalFlow {
                folder: folder.clone(),
                ..LocalFlow::default()
            });
            entry.steps.push(LocalFlowStep {
                path: path.to_string(),
                name: non_empty(yaml_str(&yaml, "name"), &resource.name),
                step_type: flow_step_type(&yaml),
                prompt: replace_resource_names_with_ids(
                    &yaml_str(&yaml, "prompt"),
                    prompt_reference_maps,
                    Some(folder.as_str()),
                ),
                asr_biasing: Some(step_asr_config(yaml.get("asr_biasing"))),
                dtmf_config: Some(step_dtmf_config(yaml.get("dtmf_config"))),
                conditions: local_conditions(&yaml),
                extracted_entities: yaml_string_list(yaml.get("extracted_entities")),
                position: step_position(yaml.get("position")),
            });
        } else if path.starts_with("flows/")
            && path.contains("/function_steps/")
            && path.ends_with(".py")
        {
            let Some(folder) = flow_folder_from_path(path) else {
                continue;
            };
            let entry = flows.entry(folder.clone()).or_insert_with(|| LocalFlow {
                folder,
                ..LocalFlow::default()
            });
            let content = resource_content(resource);
            entry.function_steps.push(LocalFunctionStep {
                path: path.to_string(),
                name: function_name_from_path(path),
                content: content.to_string(),
                code: replace_flow_import_names_with_ids(
                    &try_function_code_from_local_content(path, content)?,
                    flow_import_path_maps,
                ),
                position: None,
            });
        } else if path.starts_with("flows/")
            && path.contains("/functions/")
            && path.ends_with(".py")
        {
            let Some(folder) = flow_folder_from_path(path) else {
                continue;
            };
            let content = resource_content(resource);
            let code = replace_flow_import_names_with_ids(
                &try_function_code_from_local_content(path, content)?,
                flow_import_path_maps,
            );
            let entry = flows.entry(folder.clone()).or_insert_with(|| LocalFlow {
                folder,
                ..LocalFlow::default()
            });
            entry.transition_functions.push(LocalTransitionFunction {
                path: path.to_string(),
                name: function_name_from_path(path),
                content: content.to_string(),
                description: infer_function_description(content),
                code,
            });
        }
    }

    let mut flows = flows
        .into_values()
        .filter(|flow| !flow.name.is_empty())
        .collect::<Vec<_>>();
    flows.sort_by(|left, right| left.config_path.cmp(&right.config_path));
    Ok(flows)
}

pub(super) fn ordered_flow_steps(flow: &LocalFlow) -> Vec<&LocalFlowStep> {
    let mut steps = flow.steps.iter().collect::<Vec<_>>();
    steps.sort_by(|left, right| {
        let left_start = left.name == flow.start_step;
        let right_start = right.name == flow.start_step;
        right_start
            .cmp(&left_start)
            .then_with(|| left.path.cmp(&right.path))
    });
    steps
}

pub(super) fn ordered_function_steps(flow: &LocalFlow) -> Vec<&LocalFunctionStep> {
    let mut steps = flow.function_steps.iter().collect::<Vec<_>>();
    steps.sort_by(|left, right| left.path.cmp(&right.path));
    steps
}

pub(super) fn ordered_transition_functions(flow: &LocalFlow) -> Vec<&LocalTransitionFunction> {
    let mut functions = flow.transition_functions.iter().collect::<Vec<_>>();
    functions.sort_by(|left, right| left.path.cmp(&right.path));
    functions
}

pub(super) fn function_step_latency_control(
    step: &LocalFunctionStep,
    known_function: Option<&Value>,
) -> FunctionCreateLatencyControl {
    let local = local_latency_control_from_code(&step.content, known_function);
    function_create_latency_control(&local).unwrap_or_default()
}

pub(super) fn remote_flows_by_name(projection: &Value) -> HashMap<String, RemoteFlow> {
    let mut flows = HashMap::new();
    let Some(entities) = projection
        .get("flows")
        .and_then(|flows| flows.get("flows"))
        .and_then(|flows| flows.get("entities"))
        .and_then(Value::as_object)
    else {
        return flows;
    };
    for (id, flow) in entities {
        let name = flow
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id.as_str())
            .to_string();
        let remote = RemoteFlow {
            id: id.clone(),
            name: name.clone(),
            description: json_string(flow, &["description"]),
            start_step_id: json_string(flow, &["startStepId", "start_step_id"]),
            steps_by_name: remote_flow_steps_by_name(flow),
            function_steps_by_name: remote_function_steps_by_name(flow),
            transition_functions_by_name: remote_transition_functions_by_name(flow),
        };
        flows.insert(name, remote);
    }
    flows
}

fn remote_flow_steps_by_name(flow: &Value) -> HashMap<String, RemoteFlowStep> {
    let mut steps = HashMap::new();
    let Some(entities) = flow
        .get("steps")
        .and_then(|steps| steps.get("entities"))
        .and_then(Value::as_object)
    else {
        return steps;
    };
    for (id, step) in entities {
        let type_name = json_string(step, &["type"]);
        if type_name == "function_step" {
            continue;
        }
        let step_type = flow_step_type_from_str(type_name.as_str());
        let name = json_string(step, &["name"]);
        if name.is_empty() {
            continue;
        }
        steps.insert(
            name.clone(),
            RemoteFlowStep {
                id: id.clone(),
                name,
                step_type,
                prompt: json_string(step, &["prompt"]),
                asr_biasing: json_step_asr_config(step.get("asrBiasing")),
                dtmf_config: json_step_dtmf_config(step.get("dtmfConfig")),
                conditions_by_name: remote_conditions_by_name(step),
                extracted_entities: json_object_true_keys(
                    step.get("references")
                        .and_then(|references| references.get("extractedEntities"))
                        .or_else(|| {
                            step.get("references")
                                .and_then(|references| references.get("extracted_entities"))
                        }),
                ),
                position: json_step_position(step.get("position")),
            },
        );
    }
    steps
}

fn remote_function_steps_by_name(flow: &Value) -> HashMap<String, RemoteFunctionStep> {
    let mut steps = HashMap::new();
    let Some(entities) = flow
        .get("steps")
        .and_then(|steps| steps.get("entities"))
        .and_then(Value::as_object)
    else {
        return steps;
    };
    for (id, step) in entities {
        if json_string(step, &["type"]) != "function_step" {
            continue;
        }
        let name = json_string(step, &["name"]);
        if name.is_empty() {
            continue;
        }
        steps.insert(
            name.clone(),
            RemoteFunctionStep {
                id: id.clone(),
                name,
                code: step
                    .get("function")
                    .and_then(|function| function.get("code"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                function: step.get("function").cloned().unwrap_or(Value::Null),
                position: json_step_position(step.get("position")),
            },
        );
    }
    steps
}

fn remote_transition_functions_by_name(flow: &Value) -> HashMap<String, RemoteTransitionFunction> {
    let mut functions = HashMap::new();
    let Some(entities) = flow
        .get("transitionFunctions")
        .or_else(|| flow.get("transition_functions"))
        .and_then(|functions| functions.get("entities"))
        .and_then(Value::as_object)
    else {
        return functions;
    };
    for (id, function) in entities {
        if function
            .get("archived")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let name = json_string(function, &["name"]);
        if name.is_empty() {
            continue;
        }
        functions.insert(
            name.clone(),
            RemoteTransitionFunction {
                id: id.clone(),
                name,
                description: json_string(function, &["description"]),
                code: json_string(function, &["code"]),
                raw: function.clone(),
            },
        );
    }
    functions
}

fn remote_conditions_by_name(step: &Value) -> HashMap<String, RemoteCondition> {
    let mut conditions = HashMap::new();
    let Some(items) = step.get("conditions").and_then(Value::as_array) else {
        return conditions;
    };
    for item in items {
        let id = json_string(item, &["id"]);
        let Some(config) = item.get("config") else {
            continue;
        };
        if config.get("$case").and_then(Value::as_str) != Some("exitFlowCondition") {
            continue;
        }
        let value = config.get("value").unwrap_or(config);
        let details = value.get("details").unwrap_or(&Value::Null);
        let name = json_string(details, &["label"]);
        if name.is_empty() {
            continue;
        }
        conditions.insert(
            name.clone(),
            RemoteCondition {
                id,
                condition: LocalCondition {
                    name,
                    description: json_string(details, &["description"]),
                    condition_type: "exit_flow_condition".to_string(),
                    required_entities: json_string_list(details.get("requiredEntities")),
                    ingress: non_empty(json_string(details, &["ingressPosition"]), "top"),
                    position: json_step_position(details.get("position")),
                    exit_flow_position: json_step_position(value.get("exitFlowPosition")),
                },
            },
        );
    }
    conditions
}

fn flow_step_type_from_str(value: &str) -> FlowStepType {
    match value {
        "default_step" => FlowStepType::Default,
        _ => FlowStepType::Advanced,
    }
}

fn json_string(value: &Value, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .unwrap_or_default()
        .to_string()
}

fn json_bool(value: Option<&Value>, keys: &[&str], default: bool) -> bool {
    keys.iter()
        .find_map(|key| {
            value
                .and_then(|value| value.get(*key))
                .and_then(Value::as_bool)
        })
        .unwrap_or(default)
}

fn json_i32(value: Option<&Value>, keys: &[&str], default: i32) -> i32 {
    keys.iter()
        .find_map(|key| {
            value
                .and_then(|value| value.get(*key))
                .and_then(Value::as_i64)
        })
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or(default)
}

fn json_string_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToString::to_string)
        .collect()
}

fn json_object_true_keys(value: Option<&Value>) -> Vec<String> {
    let mut keys = value
        .and_then(Value::as_object)
        .map(|object| {
            object
                .iter()
                .filter(|(_, value)| value.as_bool().unwrap_or(true))
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    keys.sort();
    keys
}

fn json_step_asr_config(config: Option<&Value>) -> StepAsrConfig {
    StepAsrConfig {
        alphanumeric: json_bool(config, &["alphanumeric"], false),
        name_spelling: json_bool(config, &["nameSpelling", "name_spelling"], false),
        numeric: json_bool(config, &["numeric"], false),
        party_size: json_bool(config, &["partySize", "party_size"], false),
        precise_date: json_bool(config, &["preciseDate", "precise_date"], false),
        relative_date: json_bool(config, &["relativeDate", "relative_date"], false),
        single_number: json_bool(config, &["singleNumber", "single_number"], false),
        time: json_bool(config, &["time"], false),
        yes_no: json_bool(config, &["yesNo", "yes_no"], false),
        address: json_bool(config, &["address"], false),
        custom_keywords: json_string_list(config.and_then(|config| {
            config
                .get("customKeywords")
                .or_else(|| config.get("custom_keywords"))
        })),
        is_enabled: json_bool(config, &["isEnabled", "is_enabled"], false),
    }
}

fn json_step_dtmf_config(config: Option<&Value>) -> StepDtmfConfig {
    StepDtmfConfig {
        is_enabled: json_bool(config, &["isEnabled", "is_enabled"], false),
        inter_digit_timeout: json_i32(config, &["interDigitTimeout", "inter_digit_timeout"], 0),
        max_digits: json_i32(config, &["maxDigits", "max_digits"], 0),
        end_key: non_empty(
            config
                .and_then(|config| config.get("endKey").or_else(|| config.get("end_key")))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            "#",
        ),
        collect_while_agent_speaking: json_bool(
            config,
            &["collectWhileAgentSpeaking", "collect_while_agent_speaking"],
            false,
        ),
        is_pii: json_bool(config, &["isPii", "is_pii"], false),
    }
}

fn json_step_position(value: Option<&Value>) -> Option<StepPosition> {
    let value = value?;
    Some(StepPosition {
        x: value.get("x").and_then(Value::as_f64).unwrap_or_default() as f32,
        y: value.get("y").and_then(Value::as_f64).unwrap_or_default() as f32,
    })
}

fn resource_yaml(resource: &Resource) -> Option<serde_yaml::Value> {
    serde_yaml::from_str(resource_content(resource)).ok()
}

fn resource_content(resource: &Resource) -> &str {
    resource
        .payload
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn flow_folder_from_path(path: &str) -> Option<String> {
    let mut parts = path.split('/');
    (parts.next()? == "flows").then_some(parts.next()?.to_string())
}

fn function_name_from_path(path: &str) -> String {
    path.rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".py"))
        .unwrap_or(path)
        .to_string()
}

fn flow_step_type(yaml: &serde_yaml::Value) -> FlowStepType {
    match yaml
        .get("step_type")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or("advanced_step")
    {
        "default_step" => FlowStepType::Default,
        _ => FlowStepType::Advanced,
    }
}

fn local_conditions(yaml: &serde_yaml::Value) -> Vec<LocalCondition> {
    yaml.get("conditions")
        .and_then(serde_yaml::Value::as_sequence)
        .into_iter()
        .flatten()
        .map(|condition| LocalCondition {
            name: yaml_str(condition, "name"),
            description: yaml_str(condition, "description"),
            condition_type: yaml_str(condition, "condition_type"),
            required_entities: yaml_string_list(condition.get("required_entities")),
            ingress: non_empty(
                condition
                    .get("ingress")
                    .or_else(|| condition.get("ingress_position"))
                    .and_then(serde_yaml::Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                "top",
            ),
            position: step_position(condition.get("position")),
            exit_flow_position: step_position(condition.get("exit_flow_position")),
        })
        .collect()
}

fn step_asr_config(config: Option<&serde_yaml::Value>) -> StepAsrConfig {
    StepAsrConfig {
        alphanumeric: yaml_bool(config, "alphanumeric", false),
        name_spelling: yaml_bool(config, "name_spelling", false),
        numeric: yaml_bool(config, "numeric", false),
        party_size: yaml_bool(config, "party_size", false),
        precise_date: yaml_bool(config, "precise_date", false),
        relative_date: yaml_bool(config, "relative_date", false),
        single_number: yaml_bool(config, "single_number", false),
        time: yaml_bool(config, "time", false),
        yes_no: yaml_bool(config, "yes_no", false),
        address: yaml_bool(config, "address", false),
        custom_keywords: yaml_string_list(config.and_then(|c| c.get("custom_keywords"))),
        is_enabled: yaml_bool(config, "is_enabled", false),
    }
}

fn step_dtmf_config(config: Option<&serde_yaml::Value>) -> StepDtmfConfig {
    StepDtmfConfig {
        is_enabled: yaml_bool(config, "is_enabled", false),
        inter_digit_timeout: yaml_i32(config, "inter_digit_timeout", 0),
        max_digits: yaml_i32(config, "max_digits", 0),
        end_key: non_empty(yaml_string(config, "end_key"), "#"),
        collect_while_agent_speaking: yaml_bool(config, "collect_while_agent_speaking", false),
        is_pii: yaml_bool(config, "is_pii", false),
    }
}

pub(super) fn default_dtmf_config() -> StepDtmfConfig {
    step_dtmf_config(None)
}

fn step_position(value: Option<&serde_yaml::Value>) -> Option<StepPosition> {
    let value = value?;
    Some(StepPosition {
        x: value
            .get("x")
            .and_then(serde_yaml::Value::as_f64)
            .unwrap_or_default() as f32,
        y: value
            .get("y")
            .and_then(serde_yaml::Value::as_f64)
            .unwrap_or_default() as f32,
    })
}

pub(super) fn default_step_position(index: usize) -> StepPosition {
    StepPosition {
        x: (index as f32) * 600.0,
        y: 0.0,
    }
}

fn yaml_bool(config: Option<&serde_yaml::Value>, key: &str, default: bool) -> bool {
    config
        .and_then(|config| config.get(key))
        .and_then(serde_yaml::Value::as_bool)
        .unwrap_or(default)
}

fn yaml_i32(config: Option<&serde_yaml::Value>, key: &str, default: i32) -> i32 {
    config
        .and_then(|config| config.get(key))
        .and_then(serde_yaml::Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or(default)
}

fn yaml_string(config: Option<&serde_yaml::Value>, key: &str) -> String {
    config
        .and_then(|config| config.get(key))
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn yaml_string_list(value: Option<&serde_yaml::Value>) -> Vec<String> {
    value
        .and_then(serde_yaml::Value::as_sequence)
        .map(|items| {
            items
                .iter()
                .filter_map(serde_yaml::Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn non_empty(value: String, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value
    }
}
