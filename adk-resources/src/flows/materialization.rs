use crate::flows::local::FlowStepType;
use crate::flows::projection::{
    ProjectionCondition, ProjectionFlow, ProjectionFlowStep, projection_flows,
};
use crate::functions;
use crate::materialization::{
    FlowImportPathMaps, insert_content_resource, insert_yaml_resource,
    replace_flow_import_ids_with_names,
};
use crate::projection::projection_entity_values;
use crate::{CommandGenError, clean_name};
use adk_protobuf::flows::StepPosition;
use adk_types::ResourceMap;
use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
struct FlowConfigYaml {
    name: String,
    description: String,
    start_step: String,
}

#[derive(Serialize)]
struct FlowStepYaml {
    step_type: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    asr_biasing: Option<AsrBiasingYaml>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dtmf_config: Option<DtmfConfigYaml>,
    #[serde(skip_serializing_if = "Option::is_none")]
    conditions: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    extracted_entities: Option<Value>,
    prompt: String,
}

#[derive(Serialize)]
struct AsrBiasingYaml {
    is_enabled: bool,
    alphanumeric: bool,
    name_spelling: bool,
    numeric: bool,
    party_size: bool,
    precise_date: bool,
    relative_date: bool,
    single_number: bool,
    time: bool,
    yes_no: bool,
    address: bool,
    custom_keywords: Vec<String>,
}

#[derive(Serialize)]
struct DtmfConfigYaml {
    is_enabled: bool,
    inter_digit_timeout: i32,
    max_digits: i32,
    end_key: String,
    collect_while_agent_speaking: bool,
    is_pii: bool,
}

pub(crate) fn insert_flow_resources(
    map: &mut ResourceMap,
    projection: &Value,
    flow_import_path_maps: &FlowImportPathMaps,
) -> Result<(), CommandGenError> {
    for flow in projection_flows(projection) {
        insert_flow_resource(map, &flow, flow_import_path_maps)?;
    }

    Ok(())
}

pub(crate) fn flow_entries(projection: &Value) -> Vec<(String, Value)> {
    projection_entity_values(projection, &["flows", "flows"])
}

fn insert_flow_resource(
    map: &mut ResourceMap,
    flow: &ProjectionFlow,
    flow_import_path_maps: &FlowImportPathMaps,
) -> Result<(), CommandGenError> {
    let folder = clean_name(&flow.name, true);
    let start_step = flow.start_step_name_or_id().to_string();

    let flow_config = FlowConfigYaml {
        name: flow.name.clone(),
        description: flow.description.clone(),
        start_step,
    };
    let flow_config_path = format!("flows/{folder}/flow_config.yaml");
    let flow_name = flow_config.name.clone();
    insert_yaml_resource(map, &flow_config_path, &flow.id, &flow_name, flow_config)?;

    for step in &flow.steps {
        insert_flow_step_resource(map, &folder, step)?;
    }

    for step in &flow.function_steps {
        let code = replace_flow_import_ids_with_names(
            &functions::function_raw_content(&step.function),
            flow_import_path_maps,
        );
        let file_path = format!(
            "flows/{folder}/function_steps/{}.py",
            clean_name(&step.name, true)
        );
        let resource_id = format!("{}_{}", folder, step.entity_key);
        insert_content_resource(map, &file_path, &resource_id, &step.name, code)?;
    }

    for function in &flow.transition_functions {
        let function_name = function.name.clone();
        let file_name = clean_name(&function_name, true);
        let file_path = format!("flows/{folder}/functions/{file_name}.py");
        let content = replace_flow_import_ids_with_names(
            &functions::function_raw_content(&function.raw),
            flow_import_path_maps,
        );
        insert_content_resource(map, &file_path, &function.id, &function_name, content)?;
    }

    Ok(())
}

fn insert_flow_step_resource(
    map: &mut ResourceMap,
    folder: &str,
    step: &ProjectionFlowStep,
) -> Result<(), CommandGenError> {
    let value = if step.step_type == FlowStepType::Advanced {
        FlowStepYaml {
            step_type: "advanced_step".to_string(),
            name: step.name.clone(),
            asr_biasing: Some(asr_biasing_yaml(
                step.raw
                    .get("asrBiasing")
                    .or_else(|| step.raw.get("asr_biasing")),
            )),
            dtmf_config: Some(dtmf_config_yaml(
                step.raw
                    .get("dtmfConfig")
                    .or_else(|| step.raw.get("dtmf_config")),
            )),
            conditions: None,
            extracted_entities: None,
            prompt: step.prompt.clone(),
        }
    } else {
        FlowStepYaml {
            step_type: "default_step".to_string(),
            name: step.name.clone(),
            asr_biasing: None,
            dtmf_config: None,
            conditions: Some(flow_conditions_yaml(&step.conditions)),
            extracted_entities: Some(Value::Array(
                step.extracted_entities
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            )),
            prompt: step.prompt.clone(),
        }
    };
    let file_path = format!("flows/{folder}/steps/{}.yaml", clean_name(&step.name, true));
    let resource_id = format!("{}_{}", folder, step.entity_key);
    insert_yaml_resource(map, &file_path, &resource_id, &step.name, value)
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

fn flow_conditions_yaml(conditions: &[ProjectionCondition]) -> Value {
    let items = conditions
        .iter()
        .map(|condition| {
            let condition = &condition.condition;
            let mut out = serde_json::json!({
                "name": condition.name,
                "condition_type": "exit_flow_condition",
                "description": condition.description,
                "required_entities": condition.required_entities,
            });
            if !condition.ingress.is_empty() {
                out["ingress_position"] = Value::String(condition.ingress.clone());
            }
            if let Some(position) = condition.position {
                out["position"] = step_position_yaml(position);
            }
            if let Some(position) = condition.exit_flow_position {
                out["exit_flow_position"] = step_position_yaml(position);
            }
            out
        })
        .collect::<Vec<_>>();
    Value::Array(items)
}

fn step_position_yaml(position: StepPosition) -> Value {
    serde_json::json!({
        "x": position.x,
        "y": position.y,
    })
}
