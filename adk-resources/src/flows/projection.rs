//! Shared typed view over the backend flow projection.
//!
//! Most resource families can compare local resources against a flat
//! `name -> id` map or reuse their local item type for projection data. Flows
//! are nested enough that command generation and materialization both need a
//! normalized projection view: flows contain ordinary steps, no-code
//! conditions, function steps, transition functions, ASR/DTMF configs,
//! positions, and archived transition-function filtering.

use crate::flows::local::{FlowStepType, LocalCondition};
use crate::projection::{projection_entity_refs, projection_entity_refs_at};
use adk_protobuf::flows::{StepAsrConfig, StepDtmfConfig, StepPosition};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub(crate) struct ProjectionFlow {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) start_step_id: String,
    pub(crate) steps: Vec<ProjectionFlowStep>,
    pub(crate) steps_by_name: HashMap<String, ProjectionFlowStep>,
    pub(crate) function_steps: Vec<ProjectionFunctionStep>,
    pub(crate) function_steps_by_name: HashMap<String, ProjectionFunctionStep>,
    pub(crate) transition_functions: Vec<ProjectionTransitionFunction>,
    pub(crate) transition_functions_by_name: HashMap<String, ProjectionTransitionFunction>,
}

impl ProjectionFlow {
    pub(crate) fn start_step_name_or_id(&self) -> &str {
        if self.start_step_id.is_empty() {
            return "";
        }
        self.steps_by_name
            .values()
            .find(|step| self.step_id_matches_start_step(&step.id))
            .map(|step| step.name.as_str())
            .or_else(|| {
                self.function_steps_by_name
                    .values()
                    .find(|step| self.step_id_matches_start_step(&step.id))
                    .map(|step| step.name.as_str())
            })
            .unwrap_or(self.start_step_id.as_str())
    }

    fn step_id_matches_start_step(&self, step_id: &str) -> bool {
        let normalized_id = step_id
            .strip_prefix(&format!("{}_", self.name))
            .unwrap_or(step_id);
        normalized_id == self.start_step_id
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ProjectionFlowStep {
    pub(crate) entity_key: String,
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) step_type: FlowStepType,
    pub(crate) prompt: String,
    pub(crate) asr_biasing: StepAsrConfig,
    pub(crate) dtmf_config: StepDtmfConfig,
    pub(crate) conditions: Vec<ProjectionCondition>,
    pub(crate) conditions_by_name: HashMap<String, ProjectionCondition>,
    pub(crate) extracted_entities: Vec<String>,
    pub(crate) position: Option<StepPosition>,
    pub(crate) raw: JsonValue,
}

#[derive(Debug, Clone)]
pub(crate) struct ProjectionFunctionStep {
    pub(crate) entity_key: String,
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) code: String,
    pub(crate) function: JsonValue,
    pub(crate) position: Option<StepPosition>,
}

#[derive(Debug, Clone)]
pub(crate) struct ProjectionTransitionFunction {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) code: String,
    pub(crate) raw: JsonValue,
}

#[derive(Debug, Clone)]
pub(crate) struct ProjectionCondition {
    pub(crate) id: String,
    pub(crate) condition: LocalCondition,
}

pub(crate) fn projection_flows(projection: &JsonValue) -> Vec<ProjectionFlow> {
    projection_entity_refs(projection, &["flows", "flows"])
        .into_iter()
        .map(|(id, flow)| projection_flow_from_json(&id, flow))
        .collect()
}

pub(crate) fn projection_flows_by_name(projection: &JsonValue) -> HashMap<String, ProjectionFlow> {
    projection_flows(projection)
        .into_iter()
        .map(|flow| (flow.name.clone(), flow))
        .collect()
}

fn projection_flow_from_json(id: &str, flow: &JsonValue) -> ProjectionFlow {
    let name = flow
        .get("name")
        .and_then(JsonValue::as_str)
        .unwrap_or(id)
        .to_string();
    let steps = projection_flow_steps(flow);
    let function_steps = projection_function_steps(flow);
    let transition_functions = projection_transition_functions(flow);
    ProjectionFlow {
        id: id.to_string(),
        name,
        description: json_string(flow, &["description"]),
        start_step_id: json_string(flow, &["startStepId", "start_step_id"]),
        steps_by_name: steps
            .iter()
            .map(|step| (step.name.clone(), step.clone()))
            .collect(),
        steps,
        function_steps_by_name: function_steps
            .iter()
            .map(|step| (step.name.clone(), step.clone()))
            .collect(),
        function_steps,
        transition_functions_by_name: transition_functions
            .iter()
            .map(|function| (function.name.clone(), function.clone()))
            .collect(),
        transition_functions,
    }
}

fn projection_flow_steps(flow: &JsonValue) -> Vec<ProjectionFlowStep> {
    let mut steps = Vec::new();
    let Some(steps_value) = flow.get("steps") else {
        return steps;
    };
    for (id, step) in projection_entity_refs_at(steps_value) {
        let type_name = json_string(step, &["type"]);
        if type_name == "function_step" {
            continue;
        }
        let step_type = flow_step_type_from_str(type_name.as_str());
        let name = json_string(step, &["name"]);
        if name.is_empty() {
            continue;
        }
        let conditions = projection_conditions(step);
        steps.push(ProjectionFlowStep {
            entity_key: id.clone(),
            id: projection_step_id(&id, step),
            name,
            step_type,
            prompt: json_string(step, &["prompt"]).trim().to_string(),
            asr_biasing: json_step_asr_config(step.get("asrBiasing")),
            dtmf_config: json_step_dtmf_config(step.get("dtmfConfig")),
            conditions_by_name: conditions
                .iter()
                .map(|condition| (condition.condition.name.clone(), condition.clone()))
                .collect(),
            conditions,
            extracted_entities: json_object_true_keys(
                step.get("references")
                    .and_then(|references| references.get("extractedEntities"))
                    .or_else(|| {
                        step.get("references")
                            .and_then(|references| references.get("extracted_entities"))
                    }),
            ),
            position: json_step_position(step.get("position")),
            raw: step.clone(),
        });
    }
    steps
}

fn projection_function_steps(flow: &JsonValue) -> Vec<ProjectionFunctionStep> {
    let mut steps = Vec::new();
    let Some(steps_value) = flow.get("steps") else {
        return steps;
    };
    for (id, step) in projection_entity_refs_at(steps_value) {
        if json_string(step, &["type"]) != "function_step" {
            continue;
        }
        let name = json_string(step, &["name"]);
        if name.is_empty() {
            continue;
        }
        steps.push(ProjectionFunctionStep {
            entity_key: id.clone(),
            id: projection_step_id(&id, step),
            name,
            code: step
                .get("function")
                .and_then(|function| function.get("code"))
                .and_then(JsonValue::as_str)
                .unwrap_or_default()
                .to_string(),
            function: step.get("function").cloned().unwrap_or(JsonValue::Null),
            position: json_step_position(step.get("position")),
        });
    }
    steps
}

fn projection_step_id(entity_key: &str, step: &JsonValue) -> String {
    non_empty(json_string(step, &["id"]), entity_key)
}

fn projection_transition_functions(flow: &JsonValue) -> Vec<ProjectionTransitionFunction> {
    let mut functions = Vec::new();
    for collection in [
        flow.get("transitionFunctions"),
        flow.get("transition_functions"),
    ]
    .into_iter()
    .flatten()
    {
        for (id, function) in projection_entity_refs_at(collection) {
            if function
                .get("archived")
                .and_then(JsonValue::as_bool)
                .unwrap_or(false)
            {
                continue;
            }
            let name = json_string(function, &["name"]);
            if name.is_empty() {
                continue;
            }
            functions.push(ProjectionTransitionFunction {
                id,
                name,
                description: json_string(function, &["description"]),
                code: json_string(function, &["code"]),
                raw: function.clone(),
            });
        }
    }
    functions
}

fn projection_conditions(step: &JsonValue) -> Vec<ProjectionCondition> {
    let mut conditions = Vec::new();
    let Some(items) = step.get("conditions").and_then(JsonValue::as_array) else {
        return conditions;
    };
    for item in items {
        let id = json_string(item, &["id"]);
        let Some(config) = item.get("config") else {
            continue;
        };
        if config.get("$case").and_then(JsonValue::as_str) != Some("exitFlowCondition") {
            continue;
        }
        let value = config.get("value").unwrap_or(config);
        let details = value.get("details").unwrap_or(&JsonValue::Null);
        let name = json_string(details, &["label"]);
        if name.is_empty() {
            continue;
        }
        conditions.push(ProjectionCondition {
            id,
            condition: LocalCondition {
                name,
                description: json_string(details, &["description"]),
                condition_type: "exit_flow_condition".to_string(),
                child_step: String::new(),
                required_entities: json_string_list(details.get("requiredEntities")),
                ingress: non_empty(json_string(details, &["ingressPosition"]), "top"),
                position: json_step_position(details.get("position")),
                exit_flow_position: json_step_position(value.get("exitFlowPosition")),
            },
        });
    }
    conditions
}

fn flow_step_type_from_str(value: &str) -> FlowStepType {
    match value {
        "default_step" => FlowStepType::Default,
        _ => FlowStepType::Advanced,
    }
}

fn json_string(value: &JsonValue, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(JsonValue::as_str))
        .unwrap_or_default()
        .to_string()
}

fn json_bool(value: Option<&JsonValue>, keys: &[&str], default: bool) -> bool {
    keys.iter()
        .find_map(|key| {
            value
                .and_then(|value| value.get(*key))
                .and_then(JsonValue::as_bool)
        })
        .unwrap_or(default)
}

fn json_i32(value: Option<&JsonValue>, keys: &[&str], default: i32) -> i32 {
    keys.iter()
        .find_map(|key| {
            value
                .and_then(|value| value.get(*key))
                .and_then(JsonValue::as_i64)
        })
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or(default)
}

fn json_string_list(value: Option<&JsonValue>) -> Vec<String> {
    value
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .filter_map(JsonValue::as_str)
        .map(ToString::to_string)
        .collect()
}

fn json_object_true_keys(value: Option<&JsonValue>) -> Vec<String> {
    let mut keys = value
        .and_then(JsonValue::as_object)
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

fn json_step_asr_config(config: Option<&JsonValue>) -> StepAsrConfig {
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

fn json_step_dtmf_config(config: Option<&JsonValue>) -> StepDtmfConfig {
    StepDtmfConfig {
        is_enabled: json_bool(config, &["isEnabled", "is_enabled"], false),
        inter_digit_timeout: json_i32(config, &["interDigitTimeout", "inter_digit_timeout"], 0),
        max_digits: json_i32(config, &["maxDigits", "max_digits"], 0),
        end_key: non_empty(
            config
                .and_then(|config| config.get("endKey").or_else(|| config.get("end_key")))
                .and_then(JsonValue::as_str)
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

fn non_empty(value: String, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value
    }
}

fn json_step_position(value: Option<&JsonValue>) -> Option<StepPosition> {
    let value = value?;
    Some(StepPosition {
        x: value
            .get("x")
            .and_then(JsonValue::as_f64)
            .unwrap_or_default() as f32,
        y: value
            .get("y")
            .and_then(JsonValue::as_f64)
            .unwrap_or_default() as f32,
    })
}
