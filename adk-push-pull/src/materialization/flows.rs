use super::{
    FlowImportPathMaps, insert_content_resource, insert_yaml_resource,
    replace_flow_import_ids_with_names,
};
use crate::projection::projection_entity_values;
use crate::yaml_resources::{AsrBiasingYaml, DtmfConfigYaml, FlowConfigYaml, FlowStepYaml};
use crate::{CommandGenError, clean_name, command_gen};
use adk_types::ResourceMap;
use serde_json::Value;

pub(super) fn insert_flow_resources(
    map: &mut ResourceMap,
    projection: &Value,
    flow_import_path_maps: &FlowImportPathMaps,
) -> Result<(), CommandGenError> {
    for (id, flow) in flow_entries(projection) {
        insert_flow_resource(map, &id, &flow, flow_import_path_maps)?;
    }

    Ok(())
}

pub(super) fn flow_entries(projection: &Value) -> Vec<(String, Value)> {
    projection_entity_values(projection, &["flows", "flows"])
}

fn insert_flow_resource(
    map: &mut ResourceMap,
    flow_id: &str,
    flow: &Value,
    flow_import_path_maps: &FlowImportPathMaps,
) -> Result<(), CommandGenError> {
    let name = flow
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("flow")
        .to_string();
    let folder = clean_name(&name).to_lowercase();
    let start_step_id = flow.get("startStepId").and_then(Value::as_str);
    let steps = projection_entity_values(flow, &["steps"]);
    let start_step = start_step_id
        .map(|id| python_pretty_flow_start_step(&name, id, &steps))
        .unwrap_or_default()
        .to_string();

    let flow_config = FlowConfigYaml {
        name,
        description: flow
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        start_step,
    };
    let flow_config_path = format!("flows/{folder}/flow_config.yaml");
    let flow_name = flow_config.name.clone();
    insert_yaml_resource(
        map,
        &flow_config_path,
        flow.get("id").and_then(Value::as_str).unwrap_or(flow_id),
        &flow_name,
        flow_config,
    )?;

    for (id, step) in steps {
        let step_name = step
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id.as_str())
            .to_string();
        match step.get("type").and_then(Value::as_str).unwrap_or("") {
            "function_step" => {
                let function = step.get("function").unwrap_or(&Value::Null);
                let code = replace_flow_import_ids_with_names(
                    &command_gen::functions::function_raw_content(function),
                    flow_import_path_maps,
                );
                let file_path = format!(
                    "flows/{folder}/function_steps/{}.py",
                    clean_name(&step_name).to_lowercase()
                );
                let resource_id = format!("{folder}_{id}");
                insert_content_resource(map, &file_path, &resource_id, &step_name, code)?;
            }
            "default_step" => insert_flow_step_resource(map, &folder, id, step, true)?,
            _ => insert_flow_step_resource(map, &folder, id, step, false)?,
        }
    }

    for (id, function) in projection_entity_values(flow, &["transitionFunctions"])
        .into_iter()
        .chain(projection_entity_values(flow, &["transition_functions"]))
    {
        if function
            .get("archived")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let function_name = function
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id.as_str())
            .to_string();
        let file_name = clean_name(&function_name).to_lowercase();
        let file_path = format!("flows/{folder}/functions/{file_name}.py");
        let content = replace_flow_import_ids_with_names(
            &command_gen::functions::function_raw_content(&function),
            flow_import_path_maps,
        );
        insert_content_resource(map, &file_path, &id, &function_name, content)?;
    }

    Ok(())
}

fn python_pretty_flow_start_step<'a>(
    flow_name: &str,
    start_step_id: &'a str,
    steps: &'a [(String, Value)],
) -> &'a str {
    steps
        .iter()
        .find_map(|(step_id, step)| {
            let resource_id = step
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or(step_id.as_str());
            let normalized_id = resource_id
                .strip_prefix(&format!("{flow_name}_"))
                .unwrap_or(resource_id);
            if normalized_id == start_step_id {
                step.get("name").and_then(Value::as_str)
            } else {
                None
            }
        })
        .unwrap_or(start_step_id)
}

fn insert_flow_step_resource(
    map: &mut ResourceMap,
    folder: &str,
    id: String,
    step: Value,
    is_default: bool,
) -> Result<(), CommandGenError> {
    let name = step
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(id.as_str())
        .to_string();
    let value = if !is_default {
        FlowStepYaml {
            step_type: "advanced_step".to_string(),
            name: name.clone(),
            asr_biasing: Some(asr_biasing_yaml(
                step.get("asrBiasing").or_else(|| step.get("asr_biasing")),
            )),
            dtmf_config: Some(dtmf_config_yaml(
                step.get("dtmfConfig").or_else(|| step.get("dtmf_config")),
            )),
            conditions: None,
            extracted_entities: None,
            prompt: step
                .get("prompt")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }
    } else {
        FlowStepYaml {
            step_type: "default_step".to_string(),
            name: name.clone(),
            asr_biasing: None,
            dtmf_config: None,
            conditions: Some(flow_conditions_yaml(&step)),
            extracted_entities: Some(
                step.pointer("/references/extractedEntities")
                    .or_else(|| step.pointer("/references/extracted_entities"))
                    .and_then(Value::as_object)
                    .map(|refs| {
                        refs.keys()
                            .cloned()
                            .map(Value::String)
                            .collect::<Vec<Value>>()
                    })
                    .map(Value::Array)
                    .unwrap_or_else(|| Value::Array(vec![])),
            ),
            prompt: step
                .get("prompt")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }
    };
    let file_path = format!(
        "flows/{folder}/steps/{}.yaml",
        clean_name(&name).to_lowercase()
    );
    let resource_id = format!("{folder}_{id}");
    insert_yaml_resource(map, &file_path, &resource_id, &name, value)
}

fn asr_biasing_yaml(config: Option<&Value>) -> AsrBiasingYaml {
    AsrBiasingYaml {
        is_enabled: json_bool_value(config, &["isEnabled", "is_enabled"], false),
        alphanumeric: json_bool_value(config, &["alphanumeric"], false),
        name_spelling: json_bool_value(config, &["nameSpelling", "name_spelling"], false),
        numeric: json_bool_value(config, &["numeric"], false),
        party_size: json_bool_value(config, &["partySize", "party_size"], false),
        precise_date: json_bool_value(config, &["preciseDate", "precise_date"], false),
        relative_date: json_bool_value(config, &["relativeDate", "relative_date"], false),
        single_number: json_bool_value(config, &["singleNumber", "single_number"], false),
        time: json_bool_value(config, &["time"], false),
        yes_no: json_bool_value(config, &["yesNo", "yes_no"], false),
        address: json_bool_value(config, &["address"], false),
        custom_keywords: json_string_list_value(config, &["customKeywords", "custom_keywords"]),
    }
}

fn dtmf_config_yaml(config: Option<&Value>) -> DtmfConfigYaml {
    DtmfConfigYaml {
        is_enabled: json_bool_value(config, &["isEnabled", "is_enabled"], false),
        inter_digit_timeout: json_i32_value(
            config,
            &["interDigitTimeout", "inter_digit_timeout"],
            0,
        ),
        max_digits: json_i32_value(config, &["maxDigits", "max_digits"], 0),
        end_key: json_string_value(config, &["endKey", "end_key"], ""),
        collect_while_agent_speaking: json_bool_value(
            config,
            &["collectWhileAgentSpeaking", "collect_while_agent_speaking"],
            false,
        ),
        is_pii: json_bool_value(config, &["isPii", "is_pii"], false),
    }
}

fn json_bool_value(config: Option<&Value>, keys: &[&str], default: bool) -> bool {
    keys.iter()
        .find_map(|key| config.and_then(|config| config.get(*key)))
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

fn json_i32_value(config: Option<&Value>, keys: &[&str], default: i32) -> i32 {
    keys.iter()
        .find_map(|key| config.and_then(|config| config.get(*key)))
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or(default)
}

fn json_string_value(config: Option<&Value>, keys: &[&str], default: &str) -> String {
    keys.iter()
        .find_map(|key| config.and_then(|config| config.get(*key)))
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

fn json_string_list_value(config: Option<&Value>, keys: &[&str]) -> Vec<String> {
    keys.iter()
        .find_map(|key| config.and_then(|config| config.get(*key)))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToString::to_string)
        .collect()
}

fn flow_conditions_yaml(step: &Value) -> Value {
    let items = step
        .get("conditions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|condition| {
            let config = condition.get("config")?;
            let case = config.get("$case").and_then(Value::as_str).unwrap_or("");
            if case != "exitFlowCondition" {
                return None;
            }
            let value = config.get("value").unwrap_or(&Value::Null);
            let details = value.get("details").unwrap_or(&Value::Null);
            let mut out = serde_json::json!({
                "name": details.get("label").and_then(Value::as_str).unwrap_or(""),
                "condition_type": "exit_flow_condition",
                "description": details.get("description").and_then(Value::as_str).unwrap_or(""),
                "required_entities": details.get("requiredEntities").and_then(Value::as_array).cloned().unwrap_or_default(),
            });
            if let Some(ingress) = details.get("ingressPosition").and_then(Value::as_str) {
                out["ingress_position"] = Value::String(ingress.to_string());
            }
            if let Some(position) = details.get("position") {
                out["position"] = position.clone();
            }
            if let Some(position) = value.get("exitFlowPosition") {
                out["exit_flow_position"] = position.clone();
            }
            Some(out)
        })
        .collect::<Vec<_>>();
    Value::Array(items)
}
