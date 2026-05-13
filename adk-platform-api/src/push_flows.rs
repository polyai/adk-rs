//! Push command generation for flow config, advanced/default steps, and function steps.

use crate::push_functions::function_code_from_local_content;
use crate::push_single_file_resources::CommandGroups;
use crate::{generated_replay_resource_id, push_command, random_resource_id, yaml_str};
use adk_domain::{Resource, ResourceMap};
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::flows::{
    ConditionDetails, CreateAdvancedStep, CreateFunctionStep, CreateFunctionStepDefinition,
    CreateNoCodeCondition, CreateNoCodeStep, ExitFlowCondition, FlowCreateFlow,
    NoCodeStepReferences, StepAsrConfig, StepDtmfConfig, StepPosition, StepReferences,
    create_no_code_condition, create_step,
};
use adk_protobuf::functions::FunctionCreateLatencyControl;
use adk_protobuf::{Command, Metadata};
use serde_json::{Value, json};
use std::collections::HashMap;

#[derive(Debug, Default)]
struct LocalFlow {
    folder: String,
    config_path: String,
    name: String,
    description: String,
    start_step: String,
    steps: Vec<LocalFlowStep>,
    function_steps: Vec<LocalFunctionStep>,
}

#[derive(Debug)]
struct LocalFlowStep {
    path: String,
    name: String,
    step_type: FlowStepType,
    prompt: String,
    asr_biasing: Option<StepAsrConfig>,
    dtmf_config: Option<StepDtmfConfig>,
    conditions: Vec<LocalCondition>,
    position: Option<StepPosition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlowStepType {
    Advanced,
    Default,
}

#[derive(Debug)]
struct LocalFunctionStep {
    path: String,
    name: String,
    code: String,
    position: Option<StepPosition>,
}

#[derive(Debug)]
struct LocalCondition {
    name: String,
    description: String,
    condition_type: String,
    required_entities: Vec<String>,
    ingress: String,
    position: Option<StepPosition>,
    exit_flow_position: Option<StepPosition>,
}

pub(crate) fn flow_resource_command_groups(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> CommandGroups {
    let mut groups = CommandGroups::default();
    let remote_flows = remote_flow_names(projection);

    for flow in local_flows(resources) {
        if remote_flows.contains_key(&flow.name) {
            continue;
        }
        create_flow_commands(&mut groups.creates, &flow, metadata);
    }

    groups
}

pub(crate) fn payload_json_summary(payload: &CommandPayload) -> Option<(&'static str, Value)> {
    match payload {
        CommandPayload::CreateFlow(flow) => Some(("create_flow", create_flow_to_json(flow))),
        CommandPayload::CreateStep(step) => Some(("create_step", create_step_to_json(step))),
        CommandPayload::CreateNoCodeCondition(condition) => Some((
            "create_no_code_condition",
            create_no_code_condition_to_json(condition),
        )),
        _ => None,
    }
}

fn create_flow_to_json(flow: &FlowCreateFlow) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("id".to_string(), Value::String(flow.id.clone()));
    value.insert("name".to_string(), Value::String(flow.name.clone()));
    value.insert(
        "description".to_string(),
        Value::String(flow.description.clone()),
    );
    value.insert(
        "start_step_id".to_string(),
        Value::String(flow.start_step_id.clone()),
    );
    value.insert(
        "steps".to_string(),
        Value::Array(
            flow.steps
                .iter()
                .map(create_advanced_step_to_json)
                .collect(),
        ),
    );
    if !flow.transition_functions.is_empty() {
        value.insert(
            "transition_functions".to_string(),
            Value::Array(
                flow.transition_functions
                    .iter()
                    .map(|_| Value::Object(serde_json::Map::new()))
                    .collect(),
            ),
        );
    }
    value.insert(
        "no_code_steps".to_string(),
        Value::Array(
            flow.no_code_steps
                .iter()
                .map(create_no_code_step_to_json)
                .collect(),
        ),
    );
    Value::Object(value)
}

fn create_advanced_step_to_json(step: &CreateAdvancedStep) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("id".to_string(), Value::String(step.id.clone()));
    value.insert("name".to_string(), Value::String(step.name.clone()));
    value.insert("prompt".to_string(), Value::String(step.prompt.clone()));
    value.insert(
        "position".to_string(),
        step_position_to_json(step.position.as_ref()),
    );
    value.insert(
        "references".to_string(),
        Value::Object(serde_json::Map::new()),
    );
    value.insert(
        "asr_biasing".to_string(),
        step_asr_config_to_json(step.asr_biasing.as_ref()),
    );
    value.insert(
        "dtmf_config".to_string(),
        step_dtmf_config_to_json(step.dtmf_config.as_ref()),
    );
    Value::Object(value)
}

fn create_no_code_step_to_json(step: &CreateNoCodeStep) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("flow_id".to_string(), Value::String(step.flow_id.clone()));
    value.insert("step_id".to_string(), Value::String(step.step_id.clone()));
    value.insert("name".to_string(), Value::String(step.name.clone()));
    value.insert("prompt".to_string(), Value::String(step.prompt.clone()));
    value.insert(
        "position".to_string(),
        step_position_to_json(step.position.as_ref()),
    );
    value.insert(
        "references".to_string(),
        Value::Object(serde_json::Map::new()),
    );
    Value::Object(value)
}

fn create_step_to_json(step: &adk_protobuf::flows::CreateStep) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("flow_id".to_string(), Value::String(step.flow_id.clone()));
    if let Some(create_step::Payload::FunctionStep(function_step)) = &step.payload {
        value.insert(
            "function_step".to_string(),
            create_function_step_to_json(function_step),
        );
    }
    Value::Object(value)
}

fn create_function_step_to_json(step: &CreateFunctionStep) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("id".to_string(), Value::String(step.id.clone()));
    value.insert("name".to_string(), Value::String(step.name.clone()));
    value.insert(
        "position".to_string(),
        step_position_to_json(step.position.as_ref()),
    );
    if let Some(function) = &step.function {
        value.insert(
            "function".to_string(),
            create_function_step_definition_to_json(function),
        );
    }
    Value::Object(value)
}

fn create_function_step_definition_to_json(function: &CreateFunctionStepDefinition) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("id".to_string(), Value::String(function.id.clone()));
    value.insert("name".to_string(), Value::String(function.name.clone()));
    value.insert("code".to_string(), Value::String(function.code.clone()));
    if !function.errors.is_empty() {
        value.insert(
            "errors".to_string(),
            Value::Array(function.errors.iter().map(|_| json!({})).collect()),
        );
    }
    value.insert("latency_control".to_string(), json!({}));
    Value::Object(value)
}

fn create_no_code_condition_to_json(condition: &CreateNoCodeCondition) -> Value {
    let mut value = serde_json::Map::new();
    value.insert(
        "flow_id".to_string(),
        Value::String(condition.flow_id.clone()),
    );
    value.insert(
        "step_id".to_string(),
        Value::String(condition.step_id.clone()),
    );
    value.insert(
        "condition_id".to_string(),
        Value::String(condition.condition_id.clone()),
    );
    match &condition.config {
        Some(create_no_code_condition::Config::ExitFlowCondition(exit)) => {
            value.insert(
                "exit_flow_condition".to_string(),
                exit_flow_condition_to_json(exit),
            );
        }
        Some(create_no_code_condition::Config::StepCondition(_)) => {
            value.insert(
                "step_condition".to_string(),
                Value::Object(serde_json::Map::new()),
            );
        }
        Some(create_no_code_condition::Config::NoCodeStepCondition(_)) => {
            value.insert(
                "no_code_step_condition".to_string(),
                Value::Object(serde_json::Map::new()),
            );
        }
        Some(create_no_code_condition::Config::FunctionStepCondition(_)) => {
            value.insert(
                "function_step_condition".to_string(),
                Value::Object(serde_json::Map::new()),
            );
        }
        None => {}
    }
    Value::Object(value)
}

fn exit_flow_condition_to_json(condition: &ExitFlowCondition) -> Value {
    let mut value = serde_json::Map::new();
    if let Some(details) = &condition.details {
        value.insert("details".to_string(), condition_details_to_json(details));
    }
    value.insert(
        "exit_flow_position".to_string(),
        step_position_to_json(condition.exit_flow_position.as_ref()),
    );
    Value::Object(value)
}

fn condition_details_to_json(details: &ConditionDetails) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("label".to_string(), Value::String(details.label.clone()));
    if let Some(description) = &details.description {
        value.insert(
            "description".to_string(),
            Value::String(description.clone()),
        );
    }
    if !details.required_entities.is_empty() {
        value.insert(
            "required_entities".to_string(),
            json!(details.required_entities),
        );
    }
    value.insert(
        "position".to_string(),
        step_position_to_json(details.position.as_ref()),
    );
    value.insert(
        "ingress_position".to_string(),
        Value::String(details.ingress_position.clone()),
    );
    Value::Object(value)
}

fn step_position_to_json(position: Option<&StepPosition>) -> Value {
    let Some(position) = position else {
        return Value::Object(serde_json::Map::new());
    };
    let mut value = serde_json::Map::new();
    if position.x != 0.0 {
        value.insert("x".to_string(), json!(position.x as f64));
    }
    if position.y != 0.0 {
        value.insert("y".to_string(), json!(position.y as f64));
    }
    Value::Object(value)
}

fn step_asr_config_to_json(config: Option<&StepAsrConfig>) -> Value {
    let Some(config) = config else {
        return Value::Object(serde_json::Map::new());
    };
    let mut value = serde_json::Map::new();
    for (key, enabled) in [
        ("alphanumeric", config.alphanumeric),
        ("name_spelling", config.name_spelling),
        ("numeric", config.numeric),
        ("party_size", config.party_size),
        ("precise_date", config.precise_date),
        ("relative_date", config.relative_date),
        ("single_number", config.single_number),
        ("time", config.time),
        ("yes_no", config.yes_no),
        ("address", config.address),
        ("is_enabled", config.is_enabled),
    ] {
        if enabled {
            value.insert(key.to_string(), Value::Bool(true));
        }
    }
    if !config.custom_keywords.is_empty() {
        value.insert("custom_keywords".to_string(), json!(config.custom_keywords));
    }
    Value::Object(value)
}

fn step_dtmf_config_to_json(config: Option<&StepDtmfConfig>) -> Value {
    let Some(config) = config else {
        return Value::Object(serde_json::Map::new());
    };
    let mut value = serde_json::Map::new();
    if config.is_enabled {
        value.insert("is_enabled".to_string(), Value::Bool(true));
    }
    if config.inter_digit_timeout != 0 {
        value.insert(
            "inter_digit_timeout".to_string(),
            json!(config.inter_digit_timeout),
        );
    }
    if config.max_digits != 0 {
        value.insert("max_digits".to_string(), json!(config.max_digits));
    }
    if !config.end_key.is_empty() {
        value.insert("end_key".to_string(), Value::String(config.end_key.clone()));
    }
    if config.collect_while_agent_speaking {
        value.insert(
            "collect_while_agent_speaking".to_string(),
            Value::Bool(true),
        );
    }
    if config.is_pii {
        value.insert("is_pii".to_string(), Value::Bool(true));
    }
    Value::Object(value)
}

fn create_flow_commands(out: &mut Vec<Command>, flow: &LocalFlow, metadata: &Option<Metadata>) {
    let flow_id = generated_replay_resource_id("flow", &flow.name, &flow.config_path)
        .unwrap_or_else(|| random_resource_id("FLOW_CONFIG"));
    let mut step_ids = HashMap::new();
    let mut advanced_steps = Vec::new();
    let mut no_code_steps = Vec::new();

    let ordered_steps = ordered_flow_steps(flow);
    let function_steps = ordered_function_steps(flow);

    for (index, step) in ordered_steps.iter().enumerate() {
        let step_id = generated_replay_resource_id("flow_step", &step.name, &step.path)
            .unwrap_or_else(|| random_resource_id("FLOW_STEPS"));
        step_ids.insert(step.name.clone(), step_id.clone());
        let position = Some(
            step.position
                .unwrap_or_else(|| default_step_position(index)),
        );
        match step.step_type {
            FlowStepType::Advanced => advanced_steps.push(CreateAdvancedStep {
                id: step_id,
                name: step.name.clone(),
                prompt: step.prompt.clone(),
                position,
                references: Some(StepReferences::default()),
                asr_biasing: Some(step.asr_biasing.clone().unwrap_or_default()),
                dtmf_config: Some(step.dtmf_config.clone().unwrap_or_else(default_dtmf_config)),
            }),
            FlowStepType::Default => no_code_steps.push(CreateNoCodeStep {
                flow_id: flow_id.clone(),
                step_id,
                name: step.name.clone(),
                prompt: step.prompt.clone(),
                position,
                references: Some(NoCodeStepReferences::default()),
            }),
        }
    }

    let start_step_id = step_ids
        .get(&flow.start_step)
        .cloned()
        .or_else(|| advanced_steps.first().map(|step| step.id.clone()))
        .or_else(|| no_code_steps.first().map(|step| step.step_id.clone()))
        .unwrap_or_default();

    push_command(
        out,
        metadata,
        "create_flow",
        CommandPayload::CreateFlow(FlowCreateFlow {
            id: flow_id.clone(),
            name: flow.name.clone(),
            description: flow.description.clone(),
            start_step_id,
            steps: advanced_steps,
            transition_functions: Vec::new(),
            no_code_steps,
        }),
    );

    let next_index = ordered_steps.len();
    for (offset, step) in function_steps.iter().enumerate() {
        let step_id = generated_replay_resource_id("function_step", &step.name, &step.path)
            .unwrap_or_else(|| random_resource_id("FUNCTION_STEPS"));
        let function_id = generated_replay_resource_id("function", &step.name, &step.path)
            .unwrap_or_else(|| random_resource_id("FUNCTION"));
        push_command(
            out,
            metadata,
            "create_step",
            CommandPayload::CreateStep(adk_protobuf::flows::CreateStep {
                flow_id: flow_id.clone(),
                payload: Some(create_step::Payload::FunctionStep(CreateFunctionStep {
                    id: step_id,
                    name: step.name.clone(),
                    position: Some(
                        step.position
                            .unwrap_or_else(|| default_step_position(next_index + offset)),
                    ),
                    function: Some(CreateFunctionStepDefinition {
                        id: function_id,
                        name: step.name.clone(),
                        code: step.code.clone(),
                        errors: Vec::new(),
                        latency_control: Some(FunctionCreateLatencyControl::default()),
                    }),
                })),
            }),
        );
    }

    for step in ordered_steps
        .iter()
        .filter(|step| step.step_type == FlowStepType::Default)
    {
        let Some(step_id) = step_ids.get(&step.name) else {
            continue;
        };
        let step_x = step
            .position
            .unwrap_or_else(|| {
                let index = ordered_steps
                    .iter()
                    .position(|candidate| candidate.name == step.name)
                    .unwrap_or_default();
                default_step_position(index)
            })
            .x;
        for condition in &step.conditions {
            if condition.condition_type != "exit_flow_condition" {
                continue;
            }
            let condition_id =
                generated_replay_resource_id("condition", &condition.name, &step.path)
                    .unwrap_or_else(|| random_resource_id("CONDITION"));
            push_command(
                out,
                metadata,
                "create_no_code_condition",
                CommandPayload::CreateNoCodeCondition(CreateNoCodeCondition {
                    flow_id: flow_id.clone(),
                    step_id: step_id.clone(),
                    condition_id,
                    config: Some(create_no_code_condition::Config::ExitFlowCondition(
                        ExitFlowCondition {
                            details: Some(ConditionDetails {
                                label: condition.name.clone(),
                                description: Some(condition.description.clone()),
                                required_entities: condition.required_entities.clone(),
                                position: Some(condition.position.unwrap_or(StepPosition {
                                    x: step_x,
                                    y: 250.0,
                                })),
                                ingress_position: condition.ingress.clone(),
                            }),
                            exit_flow_position: Some(condition.exit_flow_position.unwrap_or(
                                StepPosition {
                                    x: step_x,
                                    y: 500.0,
                                },
                            )),
                        },
                    )),
                }),
            );
        }
    }
}

fn local_flows(resources: &ResourceMap) -> Vec<LocalFlow> {
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
                folder,
                ..LocalFlow::default()
            });
            entry.steps.push(LocalFlowStep {
                path: path.to_string(),
                name: non_empty(yaml_str(&yaml, "name"), &resource.name),
                step_type: flow_step_type(&yaml),
                prompt: yaml_str(&yaml, "prompt"),
                asr_biasing: Some(step_asr_config(yaml.get("asr_biasing"))),
                dtmf_config: Some(step_dtmf_config(yaml.get("dtmf_config"))),
                conditions: local_conditions(&yaml),
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
            entry.function_steps.push(LocalFunctionStep {
                path: path.to_string(),
                name: function_name_from_path(path),
                code: function_code_from_local_content(resource_content(resource)),
                position: None,
            });
        }
    }

    let mut flows = flows
        .into_values()
        .filter(|flow| !flow.name.is_empty())
        .collect::<Vec<_>>();
    flows.sort_by(|left, right| left.config_path.cmp(&right.config_path));
    flows
}

fn ordered_flow_steps(flow: &LocalFlow) -> Vec<&LocalFlowStep> {
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

fn ordered_function_steps(flow: &LocalFlow) -> Vec<&LocalFunctionStep> {
    let mut steps = flow.function_steps.iter().collect::<Vec<_>>();
    steps.sort_by(|left, right| left.path.cmp(&right.path));
    steps
}

fn remote_flow_names(projection: &Value) -> HashMap<String, String> {
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
        flows.insert(name, id.clone());
    }
    flows
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
        .filter_map(|condition| {
            Some(LocalCondition {
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

fn default_dtmf_config() -> StepDtmfConfig {
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

fn default_step_position(index: usize) -> StepPosition {
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
