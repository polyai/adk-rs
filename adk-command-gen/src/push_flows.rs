//! Push command generation for flow config, advanced/default steps, and function steps.

use crate::push_functions::{
    function_code_from_local_content, function_create_latency_control,
    function_errors_update_from_projection, function_parameters_update_from_projection,
    function_update_latency_control, infer_function_description, infer_function_parameters,
    latency_control_from_projection, local_latency_control_from_code,
    variable_reference_ids_from_code,
};
use crate::push_single_file_resources::CommandGroups;
use crate::{
    PromptReferenceMaps, generated_replay_resource_id, prompt_reference_maps_from_projection,
    push_command, random_resource_id, replace_resource_names_with_ids, yaml_str,
};
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::flows::{
    ConditionDetails, CreateAdvancedStep, CreateFunctionStep, CreateFunctionStepDefinition,
    CreateNoCodeCondition, CreateNoCodeStep, DeleteNoCodeCondition, DeleteStep, ExitFlowCondition,
    FlowCreateFlow, FlowCreateTransitionFunction, FlowDeleteFlow, FlowDeleteTransitionFunction,
    FlowUpdateFlow, FlowUpdateStep, FlowUpdateStepAsrConfig, FlowUpdateStepDtmfConfig,
    FlowUpdateTransitionFunction, FlowUpdateTransitionFunctionLatencyControl, NoCodeStepReferences,
    StepAsrConfig, StepAsrConfigUpdate, StepDtmfConfig, StepDtmfConfigUpdate, StepPosition,
    StepReferences, TransitionFunctionCreateTransitionFunction, TransitionFunctionReferences,
    TransitionFunctionUpdateTransitionFunction, UpdateAdvancedStep, UpdateAsrKeywords,
    UpdateFunctionStep, UpdateFunctionStepDefinition, UpdateNoCodeCondition, UpdateNoCodeStep,
    create_no_code_condition, create_step, update_no_code_condition, update_step,
};
use adk_protobuf::functions::{FunctionCreateLatencyControl, FunctionUpdateLatencyControl};
use adk_protobuf::{Command, Metadata};
use adk_types::{Resource, ResourceMap};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Default)]
struct LocalFlow {
    folder: String,
    config_path: String,
    name: String,
    description: String,
    start_step: String,
    steps: Vec<LocalFlowStep>,
    function_steps: Vec<LocalFunctionStep>,
    transition_functions: Vec<LocalTransitionFunction>,
}

#[derive(Debug, Clone)]
struct LocalFlowStep {
    path: String,
    name: String,
    step_type: FlowStepType,
    prompt: String,
    asr_biasing: Option<StepAsrConfig>,
    dtmf_config: Option<StepDtmfConfig>,
    conditions: Vec<LocalCondition>,
    extracted_entities: Vec<String>,
    position: Option<StepPosition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlowStepType {
    Advanced,
    Default,
}

#[derive(Debug, Clone)]
struct LocalFunctionStep {
    path: String,
    name: String,
    content: String,
    code: String,
    position: Option<StepPosition>,
}

#[derive(Debug, Clone)]
struct LocalTransitionFunction {
    path: String,
    name: String,
    content: String,
    code: String,
    description: String,
}

#[derive(Debug, Clone)]
struct LocalCondition {
    name: String,
    description: String,
    condition_type: String,
    required_entities: Vec<String>,
    ingress: String,
    position: Option<StepPosition>,
    exit_flow_position: Option<StepPosition>,
}

#[derive(Debug, Default)]
struct RemoteFlow {
    id: String,
    name: String,
    description: String,
    start_step_id: String,
    steps_by_name: HashMap<String, RemoteFlowStep>,
    function_steps_by_name: HashMap<String, RemoteFunctionStep>,
    transition_functions_by_name: HashMap<String, RemoteTransitionFunction>,
}

#[derive(Debug, Clone)]
struct RemoteFlowStep {
    id: String,
    name: String,
    step_type: FlowStepType,
    prompt: String,
    asr_biasing: StepAsrConfig,
    dtmf_config: StepDtmfConfig,
    conditions_by_name: HashMap<String, RemoteCondition>,
    extracted_entities: Vec<String>,
    position: Option<StepPosition>,
}

#[derive(Debug, Clone)]
struct RemoteFunctionStep {
    id: String,
    name: String,
    code: String,
    function: Value,
    position: Option<StepPosition>,
}

#[derive(Debug, Clone)]
struct RemoteTransitionFunction {
    id: String,
    name: String,
    description: String,
    code: String,
    raw: Value,
}

#[derive(Debug, Clone)]
struct RemoteCondition {
    id: String,
    condition: LocalCondition,
}

pub(crate) fn flow_resource_command_groups(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> CommandGroups {
    let mut groups = CommandGroups::default();
    let remote_flows = remote_flows_by_name(projection);
    let prompt_reference_maps = prompt_reference_maps_from_projection(projection);
    let local_flows = local_flows(resources, &prompt_reference_maps);
    let local_flow_names = local_flows
        .iter()
        .map(|flow| flow.name.clone())
        .collect::<HashSet<_>>();
    for remote in remote_flows.values() {
        if !local_flow_names.contains(&remote.name) {
            push_command(
                &mut groups.deletes,
                metadata,
                "delete_flow",
                CommandPayload::DeleteFlow(FlowDeleteFlow {
                    flow_id: remote.id.clone(),
                }),
            );
        }
    }
    for flow in local_flows {
        if let Some(remote) = remote_flows.get(&flow.name) {
            update_flow_commands(&mut groups, &flow, remote, metadata);
        } else {
            create_flow_commands(&mut groups.creates, &flow, metadata);
        }
    }

    groups
}

pub(crate) fn payload_json_summary(payload: &CommandPayload) -> Option<(&'static str, Value)> {
    match payload {
        CommandPayload::CreateFlow(flow) => Some(("create_flow", create_flow_to_json(flow))),
        CommandPayload::CreateStep(step) => Some(("create_step", create_step_to_json(step))),
        CommandPayload::CreateFlowTransitionFunction(function) => Some((
            "create_flow_transition_function",
            create_flow_transition_function_to_json(function),
        )),
        CommandPayload::CreateNoCodeCondition(condition) => Some((
            "create_no_code_condition",
            create_no_code_condition_to_json(condition),
        )),
        CommandPayload::DeleteStep(step) => Some(("delete_step", delete_step_to_json(step))),
        CommandPayload::DeleteFlow(flow) => Some(("delete_flow", delete_flow_to_json(flow))),
        CommandPayload::DeleteFlowTransitionFunction(function) => Some((
            "delete_flow_transition_function",
            delete_flow_transition_function_to_json(function),
        )),
        CommandPayload::DeleteNoCodeCondition(condition) => Some((
            "delete_no_code_condition",
            delete_no_code_condition_to_json(condition),
        )),
        CommandPayload::UpdateFlowStep(step) => {
            Some(("update_flow_step", update_flow_step_to_json(step)))
        }
        CommandPayload::UpdateFlowTransitionFunction(function) => Some((
            "update_flow_transition_function",
            update_flow_transition_function_to_json(function),
        )),
        CommandPayload::UpdateFlowTransitionFunctionLatencyControl(update) => Some((
            "update_flow_transition_function_latency_control",
            update_flow_transition_function_latency_control_to_json(update),
        )),
        CommandPayload::UpdateNoCodeStep(step) => {
            Some(("update_no_code_step", update_no_code_step_to_json(step)))
        }
        CommandPayload::UpdateFlow(flow) => Some(("update_flow", update_flow_to_json(flow))),
        CommandPayload::UpdateFlowStepAsrConfig(config) => Some((
            "update_flow_step_asr_config",
            update_flow_step_asr_config_to_json(config),
        )),
        CommandPayload::UpdateFlowStepDtmfConfig(config) => Some((
            "update_flow_step_dtmf_config",
            update_flow_step_dtmf_config_to_json(config),
        )),
        CommandPayload::UpdateNoCodeCondition(condition) => Some((
            "update_no_code_condition",
            update_no_code_condition_to_json(condition),
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
                    .map(create_transition_function_to_json)
                    .collect(),
            ),
        );
    }
    if !flow.no_code_steps.is_empty() {
        value.insert(
            "no_code_steps".to_string(),
            Value::Array(
                flow.no_code_steps
                    .iter()
                    .map(create_no_code_step_to_json)
                    .collect(),
            ),
        );
    }
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
        no_code_step_references_to_json(step.references.as_ref()),
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
    if let Some(latency_control) = &function.latency_control {
        value.insert(
            "latency_control".to_string(),
            function_create_latency_control_to_json(latency_control),
        );
    }
    Value::Object(value)
}

fn create_flow_transition_function_to_json(function: &FlowCreateTransitionFunction) -> Value {
    let mut value = serde_json::Map::new();
    value.insert(
        "flow_id".to_string(),
        Value::String(function.flow_id.clone()),
    );
    if let Some(transition_function) = &function.transition_function {
        value.insert(
            "transition_function".to_string(),
            create_transition_function_to_json(transition_function),
        );
    }
    Value::Object(value)
}

fn function_create_latency_control_to_json(latency: &FunctionCreateLatencyControl) -> Value {
    if !latency.enabled
        && latency.initial_delay == 0
        && latency.interval == 0
        && latency.delay_responses.is_empty()
    {
        return json!({});
    }
    json!({
        "enabled": latency.enabled,
        "initial_delay": latency.initial_delay,
        "interval": latency.interval,
        "delay_responses": latency.delay_responses.iter().map(|response| {
            json!({
                "id": response.id,
                "message": response.message,
                "duration": response.duration,
            })
        }).collect::<Vec<_>>(),
    })
}

fn create_transition_function_to_json(
    function: &TransitionFunctionCreateTransitionFunction,
) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("id".to_string(), Value::String(function.id.clone()));
    value.insert("name".to_string(), Value::String(function.name.clone()));
    value.insert(
        "description".to_string(),
        Value::String(function.description.clone()),
    );
    value.insert(
        "parameters".to_string(),
        Value::Array(function.parameters.iter().map(|_| json!({})).collect()),
    );
    value.insert("code".to_string(), Value::String(function.code.clone()));
    value.insert(
        "errors".to_string(),
        Value::Array(function.errors.iter().map(|_| json!({})).collect()),
    );
    if let Some(latency_control) = &function.latency_control {
        value.insert(
            "latency_control".to_string(),
            function_create_latency_control_to_json(latency_control),
        );
    }
    value.insert(
        "references".to_string(),
        transition_function_references_to_json(function.references.as_ref()),
    );
    if let Some(archived) = function.archived {
        value.insert("archived".to_string(), Value::Bool(archived));
    }
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

fn delete_step_to_json(step: &DeleteStep) -> Value {
    json!({
        "flow_id": step.flow_id,
        "step_id": step.step_id,
    })
}

fn delete_flow_to_json(flow: &FlowDeleteFlow) -> Value {
    json!({
        "flow_id": flow.flow_id,
    })
}

fn delete_flow_transition_function_to_json(function: &FlowDeleteTransitionFunction) -> Value {
    json!({
        "flow_id": function.flow_id,
        "function_id": function.function_id,
    })
}

fn delete_no_code_condition_to_json(condition: &DeleteNoCodeCondition) -> Value {
    json!({
        "flow_id": condition.flow_id,
        "step_id": condition.step_id,
        "condition_id": condition.condition_id,
    })
}

fn update_flow_step_to_json(step: &FlowUpdateStep) -> Value {
    json!({
        "flow_id": step.flow_id,
        "step": step
            .step
            .as_ref()
            .map(update_advanced_step_to_json)
            .unwrap_or_else(|| json!({})),
    })
}

fn update_flow_transition_function_to_json(function: &FlowUpdateTransitionFunction) -> Value {
    let mut value = serde_json::Map::new();
    value.insert(
        "flow_id".to_string(),
        Value::String(function.flow_id.clone()),
    );
    if let Some(transition_function) = &function.transition_function {
        value.insert(
            "transition_function".to_string(),
            update_transition_function_to_json(transition_function),
        );
    }
    Value::Object(value)
}

fn update_flow_transition_function_latency_control_to_json(
    update: &FlowUpdateTransitionFunctionLatencyControl,
) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("flow_id".to_string(), Value::String(update.flow_id.clone()));
    if let Some(latency) = &update.latency_control {
        value.insert(
            "latency_control".to_string(),
            function_update_latency_control_to_json(latency),
        );
    }
    Value::Object(value)
}

fn function_update_latency_control_to_json(update: &FunctionUpdateLatencyControl) -> Value {
    let mut value = serde_json::Map::new();
    value.insert(
        "function_id".to_string(),
        Value::String(update.function_id.clone()),
    );
    value.insert("enabled".to_string(), Value::Bool(update.enabled));
    if let Some(initial_delay) = update.initial_delay {
        value.insert(
            "initial_delay".to_string(),
            Value::Number(initial_delay.into()),
        );
    }
    if let Some(interval) = update.interval {
        value.insert("interval".to_string(), Value::Number(interval.into()));
    }
    if let Some(delay_responses) = &update.delay_responses {
        value.insert(
            "delay_responses".to_string(),
            Value::Array(
                delay_responses
                    .delay_responses
                    .iter()
                    .map(|response| {
                        json!({
                            "id": response.id,
                            "message": response.message,
                            "duration": response.duration,
                        })
                    })
                    .collect(),
            ),
        );
    }
    Value::Object(value)
}

fn update_transition_function_to_json(
    function: &TransitionFunctionUpdateTransitionFunction,
) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("id".to_string(), Value::String(function.id.clone()));
    if let Some(name) = &function.name {
        value.insert("name".to_string(), Value::String(name.clone()));
    }
    if let Some(description) = &function.description {
        value.insert(
            "description".to_string(),
            Value::String(description.clone()),
        );
    }
    if function.parameters.is_some() {
        value.insert("parameters".to_string(), json!({}));
    }
    if let Some(code) = &function.code {
        value.insert("code".to_string(), Value::String(code.clone()));
    }
    if function.errors.is_some() {
        value.insert("errors".to_string(), json!({}));
    }
    if let Some(references) = &function.references {
        value.insert(
            "references".to_string(),
            transition_function_references_to_json(Some(references)),
        );
    }
    Value::Object(value)
}

fn update_advanced_step_to_json(step: &UpdateAdvancedStep) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("id".to_string(), Value::String(step.id.clone()));
    if let Some(name) = &step.name {
        value.insert("name".to_string(), Value::String(name.clone()));
    }
    if let Some(prompt) = &step.prompt {
        value.insert("prompt".to_string(), Value::String(prompt.clone()));
    }
    value.insert(
        "references".to_string(),
        Value::Object(serde_json::Map::new()),
    );
    Value::Object(value)
}

fn update_no_code_step_to_json(step: &UpdateNoCodeStep) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("flow_id".to_string(), Value::String(step.flow_id.clone()));
    value.insert("step_id".to_string(), Value::String(step.step_id.clone()));
    if let Some(name) = &step.name {
        value.insert("name".to_string(), Value::String(name.clone()));
    }
    if let Some(prompt) = &step.prompt {
        value.insert("prompt".to_string(), Value::String(prompt.clone()));
    }
    if let Some(position) = &step.position {
        value.insert(
            "position".to_string(),
            step_position_to_json(Some(position)),
        );
    }
    value.insert(
        "references".to_string(),
        no_code_step_references_to_json(step.references.as_ref()),
    );
    Value::Object(value)
}

fn no_code_step_references_to_json(references: Option<&NoCodeStepReferences>) -> Value {
    let Some(references) = references else {
        return Value::Object(serde_json::Map::new());
    };
    let mut value = serde_json::Map::new();
    if !references.entities.is_empty() {
        value.insert("entities".to_string(), json!(references.entities));
    }
    if !references.attributes.is_empty() {
        value.insert("attributes".to_string(), json!(references.attributes));
    }
    if !references.variables.is_empty() {
        value.insert("variables".to_string(), json!(references.variables));
    }
    if !references.extracted_entities.is_empty() {
        value.insert(
            "extracted_entities".to_string(),
            json!(references.extracted_entities),
        );
    }
    Value::Object(value)
}

fn transition_function_references_to_json(
    references: Option<&TransitionFunctionReferences>,
) -> Value {
    let Some(references) = references else {
        return Value::Object(serde_json::Map::new());
    };
    let mut value = serde_json::Map::new();
    if !references.flow_steps.is_empty() {
        value.insert("flow_steps".to_string(), json!(references.flow_steps));
    }
    if !references.variables.is_empty() {
        value.insert("variables".to_string(), json!(references.variables));
    }
    Value::Object(value)
}

fn update_flow_to_json(flow: &FlowUpdateFlow) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("flow_id".to_string(), Value::String(flow.flow_id.clone()));
    if let Some(name) = &flow.name {
        value.insert("name".to_string(), Value::String(name.clone()));
    }
    if let Some(description) = &flow.description {
        value.insert(
            "description".to_string(),
            Value::String(description.clone()),
        );
    }
    if let Some(start_step_id) = &flow.start_step_id {
        value.insert(
            "start_step_id".to_string(),
            Value::String(start_step_id.clone()),
        );
    }
    if let Some(old_flow_name) = &flow.old_flow_name {
        value.insert(
            "old_flow_name".to_string(),
            Value::String(old_flow_name.clone()),
        );
    }
    Value::Object(value)
}

fn update_flow_step_asr_config_to_json(config: &FlowUpdateStepAsrConfig) -> Value {
    json!({
        "flow_id": config.flow_id,
        "step_id": config.step_id,
        "asr_biasing": config
            .asr_biasing
            .as_ref()
            .map(step_asr_config_update_to_json)
            .unwrap_or_else(|| json!({})),
    })
}

fn step_asr_config_update_to_json(config: &StepAsrConfigUpdate) -> Value {
    let mut value = serde_json::Map::new();
    value.insert(
        "alphanumeric".to_string(),
        json!(config.alphanumeric.unwrap_or(false)),
    );
    value.insert(
        "name_spelling".to_string(),
        json!(config.name_spelling.unwrap_or(false)),
    );
    value.insert(
        "numeric".to_string(),
        json!(config.numeric.unwrap_or(false)),
    );
    value.insert(
        "party_size".to_string(),
        json!(config.party_size.unwrap_or(false)),
    );
    value.insert(
        "precise_date".to_string(),
        json!(config.precise_date.unwrap_or(false)),
    );
    value.insert(
        "relative_date".to_string(),
        json!(config.relative_date.unwrap_or(false)),
    );
    value.insert(
        "single_number".to_string(),
        json!(config.single_number.unwrap_or(false)),
    );
    value.insert("time".to_string(), json!(config.time.unwrap_or(false)));
    value.insert("yes_no".to_string(), json!(config.yes_no.unwrap_or(false)));
    value.insert(
        "address".to_string(),
        json!(config.address.unwrap_or(false)),
    );
    if let Some(custom_keywords) = &config.custom_keywords {
        value.insert(
            "custom_keywords".to_string(),
            json!({ "custom_keywords": custom_keywords.custom_keywords }),
        );
    }
    value.insert(
        "is_enabled".to_string(),
        json!(config.is_enabled.unwrap_or(false)),
    );
    Value::Object(value)
}

fn update_flow_step_dtmf_config_to_json(config: &FlowUpdateStepDtmfConfig) -> Value {
    json!({
        "flow_id": config.flow_id,
        "step_id": config.step_id,
        "dtmf_config": config
            .dtmf_config
            .as_ref()
            .map(step_dtmf_config_update_to_json)
            .unwrap_or_else(|| json!({})),
    })
}

fn step_dtmf_config_update_to_json(config: &StepDtmfConfigUpdate) -> Value {
    json!({
        "is_enabled": config.is_enabled.unwrap_or(false),
        "inter_digit_timeout": config.inter_digit_timeout.unwrap_or(0),
        "max_digits": config.max_digits.unwrap_or(0),
        "end_key": config.end_key.clone().unwrap_or_default(),
        "collect_while_agent_speaking": config.collect_while_agent_speaking.unwrap_or(false),
        "is_pii": config.is_pii.unwrap_or(false),
    })
}

fn update_no_code_condition_to_json(condition: &UpdateNoCodeCondition) -> Value {
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
        Some(update_no_code_condition::Config::ExitFlowCondition(exit)) => {
            value.insert(
                "exit_flow_condition".to_string(),
                exit_flow_condition_to_json(exit),
            );
        }
        Some(update_no_code_condition::Config::StepCondition(_)) => {
            value.insert(
                "step_condition".to_string(),
                Value::Object(serde_json::Map::new()),
            );
        }
        Some(update_no_code_condition::Config::NoCodeStepCondition(_)) => {
            value.insert(
                "no_code_step_condition".to_string(),
                Value::Object(serde_json::Map::new()),
            );
        }
        Some(update_no_code_condition::Config::FunctionStepCondition(_)) => {
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
    let transition_functions = ordered_transition_functions(flow);

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
                references: Some(no_code_step_references(step)),
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
            transition_functions: transition_functions
                .iter()
                .map(|function| transition_function_create_payload(function, None, &Value::Null))
                .collect(),
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
                        latency_control: Some(function_step_latency_control(step, None)),
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

fn transition_function_create_payload(
    function: &LocalTransitionFunction,
    id_override: Option<String>,
    projection: &Value,
) -> TransitionFunctionCreateTransitionFunction {
    let parameters = infer_function_parameters(&function.code);
    TransitionFunctionCreateTransitionFunction {
        id: id_override.unwrap_or_else(|| {
            generated_replay_resource_id("flow_transition_function", &function.name, &function.path)
                .unwrap_or_else(|| random_resource_id("FLOW_TRANSITION_FUNCTIONS"))
        }),
        name: function.name.clone(),
        description: function.description.clone(),
        parameters,
        code: function.code.clone(),
        errors: Vec::new(),
        latency_control: None,
        references: Some(TransitionFunctionReferences {
            flow_steps: HashMap::new(),
            variables: variable_reference_ids_from_code(&function.code, projection),
        }),
        archived: Some(false),
    }
}

fn transition_function_changed(
    local: &LocalTransitionFunction,
    remote: &RemoteTransitionFunction,
) -> bool {
    local.code != remote.code
        || (!local.description.is_empty() && local.description != remote.description)
        || local.name != remote.name
}

fn update_flow_commands(
    groups: &mut CommandGroups,
    flow: &LocalFlow,
    remote: &RemoteFlow,
    metadata: &Option<Metadata>,
) {
    let ordered_steps = ordered_flow_steps(flow);
    let local_step_names = ordered_steps
        .iter()
        .map(|step| step.name.clone())
        .collect::<HashSet<_>>();

    for remote_step in remote.steps_by_name.values() {
        if !local_step_names.contains(&remote_step.name) {
            match remote_step.step_type {
                FlowStepType::Advanced => push_command(
                    &mut groups.deletes,
                    metadata,
                    "delete_flow_step",
                    CommandPayload::DeleteFlowStep(adk_protobuf::flows::FlowDeleteStep {
                        flow_id: remote.id.clone(),
                        step_id: remote_step.id.clone(),
                    }),
                ),
                FlowStepType::Default => push_command(
                    &mut groups.deletes,
                    metadata,
                    "delete_no_code_step",
                    CommandPayload::DeleteNoCodeStep(adk_protobuf::flows::DeleteNoCodeStep {
                        flow_id: remote.id.clone(),
                        step_id: remote_step.id.clone(),
                    }),
                ),
            }
        }
    }

    for step in &ordered_steps {
        let Some(remote_step) = remote.steps_by_name.get(&step.name) else {
            continue;
        };
        match step.step_type {
            FlowStepType::Advanced => {
                if step.prompt != remote_step.prompt || step.name != remote_step.name {
                    push_command(
                        &mut groups.updates,
                        metadata,
                        "update_flow_step",
                        CommandPayload::UpdateFlowStep(FlowUpdateStep {
                            flow_id: remote.id.clone(),
                            step: Some(UpdateAdvancedStep {
                                id: remote_step.id.clone(),
                                name: Some(step.name.clone()),
                                prompt: Some(step.prompt.clone()),
                                references: Some(StepReferences::default()),
                            }),
                        }),
                    );
                }
            }
            FlowStepType::Default => {
                let local_condition_names = step
                    .conditions
                    .iter()
                    .map(|condition| condition.name.clone())
                    .collect::<HashSet<_>>();
                let remote_condition_names = remote_step
                    .conditions_by_name
                    .keys()
                    .cloned()
                    .collect::<HashSet<_>>();
                if step.prompt != remote_step.prompt
                    || step.name != remote_step.name
                    || step.extracted_entities != remote_step.extracted_entities
                    || local_condition_names != remote_condition_names
                {
                    push_command(
                        &mut groups.updates,
                        metadata,
                        "update_no_code_step",
                        CommandPayload::UpdateNoCodeStep(UpdateNoCodeStep {
                            flow_id: remote.id.clone(),
                            step_id: remote_step.id.clone(),
                            name: Some(step.name.clone()),
                            prompt: Some(step.prompt.clone()),
                            position: None,
                            references: Some(no_code_step_references(step)),
                        }),
                    );
                }
            }
        }
    }

    let start_step_id = local_start_step_id(flow, remote);
    if flow.name != remote.name
        || flow.description != remote.description
        || start_step_id != remote.start_step_id
    {
        push_command(
            &mut groups.updates,
            metadata,
            "update_flow",
            CommandPayload::UpdateFlow(FlowUpdateFlow {
                flow_id: remote.id.clone(),
                name: Some(flow.name.clone()),
                description: Some(flow.description.clone()),
                start_step_id: Some(start_step_id),
                old_flow_name: None,
            }),
        );
    }

    for step in &ordered_steps {
        let Some(remote_step) = remote.steps_by_name.get(&step.name) else {
            continue;
        };
        if step.step_type == FlowStepType::Advanced {
            let local_asr = step.asr_biasing.clone().unwrap_or_default();
            if local_asr != remote_step.asr_biasing {
                push_command(
                    &mut groups.updates,
                    metadata,
                    "update_flow_step_asr_config",
                    CommandPayload::UpdateFlowStepAsrConfig(FlowUpdateStepAsrConfig {
                        flow_id: remote.id.clone(),
                        step_id: remote_step.id.clone(),
                        asr_biasing: Some(step_asr_config_update(&local_asr)),
                    }),
                );
            }
            let local_dtmf = step.dtmf_config.clone().unwrap_or_else(default_dtmf_config);
            if local_dtmf != remote_step.dtmf_config {
                push_command(
                    &mut groups.updates,
                    metadata,
                    "update_flow_step_dtmf_config",
                    CommandPayload::UpdateFlowStepDtmfConfig(FlowUpdateStepDtmfConfig {
                        flow_id: remote.id.clone(),
                        step_id: remote_step.id.clone(),
                        dtmf_config: Some(step_dtmf_config_update(&local_dtmf)),
                    }),
                );
            }
        }

        if step.step_type == FlowStepType::Default {
            let local_condition_names = step
                .conditions
                .iter()
                .map(|condition| condition.name.clone())
                .collect::<HashSet<_>>();
            for remote_condition in remote_step.conditions_by_name.values() {
                if !local_condition_names.contains(&remote_condition.condition.name) {
                    push_command(
                        &mut groups.deletes,
                        metadata,
                        "delete_no_code_condition",
                        CommandPayload::DeleteNoCodeCondition(DeleteNoCodeCondition {
                            flow_id: remote.id.clone(),
                            step_id: remote_step.id.clone(),
                            condition_id: remote_condition.id.clone(),
                        }),
                    );
                }
            }
            for condition in &step.conditions {
                let Some(remote_condition) = remote_step.conditions_by_name.get(&condition.name)
                else {
                    continue;
                };
                let merged =
                    condition_with_remote_positions(condition, &remote_condition.condition);
                if merged.description != remote_condition.condition.description
                    || merged.required_entities != remote_condition.condition.required_entities
                    || merged.ingress != remote_condition.condition.ingress
                    || merged.position != remote_condition.condition.position
                    || merged.exit_flow_position != remote_condition.condition.exit_flow_position
                {
                    push_command(
                        &mut groups.updates,
                        metadata,
                        "update_no_code_condition",
                        CommandPayload::UpdateNoCodeCondition(UpdateNoCodeCondition {
                            flow_id: remote.id.clone(),
                            step_id: remote_step.id.clone(),
                            condition_id: remote_condition.id.clone(),
                            config: Some(update_no_code_condition::Config::ExitFlowCondition(
                                exit_flow_condition_from_local(&merged),
                            )),
                        }),
                    );
                }
            }
        }
    }

    let local_function_steps = flow
        .function_steps
        .iter()
        .map(|step| (step.name.clone(), step))
        .collect::<HashMap<_, _>>();

    for remote_step in remote.function_steps_by_name.values() {
        if !local_function_steps.contains_key(&remote_step.name) {
            push_command(
                &mut groups.deletes,
                metadata,
                "delete_step",
                CommandPayload::DeleteStep(DeleteStep {
                    flow_id: remote.id.clone(),
                    step_id: remote_step.id.clone(),
                }),
            );
        }
    }

    let next_position = next_function_step_position(remote);
    for step in ordered_function_steps(flow) {
        if let Some(remote_step) = remote.function_steps_by_name.get(&step.name) {
            if step.code != remote_step.code {
                push_command(
                    &mut groups.updates,
                    metadata,
                    "update_step",
                    CommandPayload::UpdateStep(adk_protobuf::flows::UpdateStep {
                        flow_id: remote.id.clone(),
                        step_id: remote_step.id.clone(),
                        payload: Some(update_step::Payload::FunctionStep(UpdateFunctionStep {
                            name: Some(step.name.clone()),
                            position: remote_step.position,
                            function: Some(UpdateFunctionStepDefinition {
                                description: None,
                                code: Some(step.code.clone()),
                                errors: None,
                                archived: None,
                                latency_control: Some(function_step_latency_control(
                                    step,
                                    Some(&remote_step.function),
                                )),
                            }),
                        })),
                    }),
                );
            }
            continue;
        }

        let step_id = generated_replay_resource_id("function_step", &step.name, &step.path)
            .unwrap_or_else(|| random_resource_id("FUNCTION_STEPS"));
        let function_id = generated_replay_resource_id("function", &step.name, &step.path)
            .unwrap_or_else(|| random_resource_id("FUNCTION"));
        push_command(
            &mut groups.creates,
            metadata,
            "create_step",
            CommandPayload::CreateStep(adk_protobuf::flows::CreateStep {
                flow_id: remote.id.clone(),
                payload: Some(create_step::Payload::FunctionStep(CreateFunctionStep {
                    id: step_id,
                    name: step.name.clone(),
                    position: Some(step.position.unwrap_or(next_position)),
                    function: Some(CreateFunctionStepDefinition {
                        id: function_id,
                        name: step.name.clone(),
                        code: step.code.clone(),
                        errors: Vec::new(),
                        latency_control: Some(function_step_latency_control(step, None)),
                    }),
                })),
            }),
        );
    }

    let local_transition_functions = flow
        .transition_functions
        .iter()
        .map(|function| (function.name.clone(), function))
        .collect::<HashMap<_, _>>();

    for remote_function in remote.transition_functions_by_name.values() {
        if !local_transition_functions.contains_key(&remote_function.name) {
            push_command(
                &mut groups.deletes,
                metadata,
                "delete_flow_transition_function",
                CommandPayload::DeleteFlowTransitionFunction(FlowDeleteTransitionFunction {
                    flow_id: remote.id.clone(),
                    function_id: remote_function.id.clone(),
                }),
            );
        }
    }

    for function in ordered_transition_functions(flow) {
        if let Some(remote_function) = remote.transition_functions_by_name.get(&function.name) {
            if transition_function_changed(function, remote_function) {
                let parameters = function_parameters_update_from_projection(&remote_function.raw)
                    .or_else(|| {
                        let params = infer_function_parameters(&function.code);
                        (!params.is_empty()).then_some(adk_protobuf::functions::ParametersUpdate {
                            parameters: params,
                        })
                    });
                push_command(
                    &mut groups.updates,
                    metadata,
                    "update_flow_transition_function",
                    CommandPayload::UpdateFlowTransitionFunction(FlowUpdateTransitionFunction {
                        flow_id: remote.id.clone(),
                        transition_function: Some(TransitionFunctionUpdateTransitionFunction {
                            id: remote_function.id.clone(),
                            name: Some(function.name.clone()),
                            description: Some(function.description.clone()),
                            parameters,
                            code: Some(function.code.clone()),
                            errors: function_errors_update_from_projection(&remote_function.raw),
                            references: None,
                        }),
                    }),
                );
            }
            let local_latency =
                local_latency_control_from_code(&function.content, Some(&remote_function.raw));
            let remote_latency = latency_control_from_projection(&remote_function.raw);
            if local_latency != remote_latency {
                push_command(
                    &mut groups.post_updates,
                    metadata,
                    "update_flow_transition_function_latency_control",
                    CommandPayload::UpdateFlowTransitionFunctionLatencyControl(
                        FlowUpdateTransitionFunctionLatencyControl {
                            flow_id: remote.id.clone(),
                            latency_control: Some(function_update_latency_control(
                                &remote_function.id,
                                &local_latency,
                            )),
                        },
                    ),
                );
            }
            continue;
        }

        let function_id = generated_replay_resource_id(
            "flow_transition_function",
            &function.name,
            &function.path,
        )
        .unwrap_or_else(|| random_resource_id("FLOW_TRANSITION_FUNCTIONS"));
        push_command(
            &mut groups.creates,
            metadata,
            "create_flow_transition_function",
            CommandPayload::CreateFlowTransitionFunction(FlowCreateTransitionFunction {
                flow_id: remote.id.clone(),
                transition_function: Some(transition_function_create_payload(
                    function,
                    Some(function_id.clone()),
                    &serde_json::Value::Null,
                )),
            }),
        );
        let local_latency = local_latency_control_from_code(&function.content, None);
        if local_latency.enabled {
            push_command(
                &mut groups.post_updates,
                metadata,
                "update_flow_transition_function_latency_control",
                CommandPayload::UpdateFlowTransitionFunctionLatencyControl(
                    FlowUpdateTransitionFunctionLatencyControl {
                        flow_id: remote.id.clone(),
                        latency_control: Some(function_update_latency_control(
                            &function_id,
                            &local_latency,
                        )),
                    },
                ),
            );
        }
    }
}

fn local_start_step_id(flow: &LocalFlow, remote: &RemoteFlow) -> String {
    if flow.start_step == remote.start_step_id {
        return flow.start_step.clone();
    }
    remote
        .steps_by_name
        .get(&flow.start_step)
        .map(|step| step.id.clone())
        .unwrap_or_else(|| flow.start_step.clone())
}

fn next_function_step_position(remote: &RemoteFlow) -> StepPosition {
    let max_x = remote
        .steps_by_name
        .values()
        .filter_map(|step| step.position)
        .chain(
            remote
                .function_steps_by_name
                .values()
                .filter_map(|step| step.position),
        )
        .map(|position| position.x)
        .fold(500.0_f32, f32::max);
    StepPosition {
        x: max_x + 400.0,
        y: 0.0,
    }
}

fn condition_with_remote_positions(
    local: &LocalCondition,
    remote: &LocalCondition,
) -> LocalCondition {
    let mut merged = local.clone();
    if merged.position.is_none() {
        merged.position = remote.position;
    }
    if merged.exit_flow_position.is_none() {
        merged.exit_flow_position = remote.exit_flow_position;
    }
    if merged.ingress.is_empty() {
        merged.ingress = remote.ingress.clone();
    }
    merged
}

fn exit_flow_condition_from_local(condition: &LocalCondition) -> ExitFlowCondition {
    ExitFlowCondition {
        details: Some(ConditionDetails {
            label: condition.name.clone(),
            description: Some(condition.description.clone()),
            required_entities: condition.required_entities.clone(),
            position: condition.position,
            ingress_position: condition.ingress.clone(),
        }),
        exit_flow_position: condition.exit_flow_position,
    }
}

fn no_code_step_references(step: &LocalFlowStep) -> NoCodeStepReferences {
    NoCodeStepReferences {
        extracted_entities: step
            .extracted_entities
            .iter()
            .map(|entity| (entity.clone(), true))
            .collect(),
        ..NoCodeStepReferences::default()
    }
}

fn step_asr_config_update(config: &StepAsrConfig) -> StepAsrConfigUpdate {
    StepAsrConfigUpdate {
        alphanumeric: Some(config.alphanumeric),
        name_spelling: Some(config.name_spelling),
        numeric: Some(config.numeric),
        party_size: Some(config.party_size),
        precise_date: Some(config.precise_date),
        relative_date: Some(config.relative_date),
        single_number: Some(config.single_number),
        time: Some(config.time),
        yes_no: Some(config.yes_no),
        address: Some(config.address),
        custom_keywords: Some(UpdateAsrKeywords {
            custom_keywords: config.custom_keywords.clone(),
        }),
        is_enabled: Some(config.is_enabled),
    }
}

fn step_dtmf_config_update(config: &StepDtmfConfig) -> StepDtmfConfigUpdate {
    StepDtmfConfigUpdate {
        is_enabled: Some(config.is_enabled),
        inter_digit_timeout: Some(config.inter_digit_timeout),
        max_digits: Some(config.max_digits),
        end_key: Some(config.end_key.clone()),
        collect_while_agent_speaking: Some(config.collect_while_agent_speaking),
        is_pii: Some(config.is_pii),
    }
}

fn local_flows(
    resources: &ResourceMap,
    prompt_reference_maps: &PromptReferenceMaps,
) -> Vec<LocalFlow> {
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
                code: function_code_from_local_content(content),
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
            let code = function_code_from_local_content(content);
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

fn ordered_transition_functions(flow: &LocalFlow) -> Vec<&LocalTransitionFunction> {
    let mut functions = flow.transition_functions.iter().collect::<Vec<_>>();
    functions.sort_by(|left, right| left.path.cmp(&right.path));
    functions
}

fn function_step_latency_control(
    step: &LocalFunctionStep,
    known_function: Option<&Value>,
) -> FunctionCreateLatencyControl {
    let local = local_latency_control_from_code(&step.content, known_function);
    function_create_latency_control(&local).unwrap_or_default()
}

fn remote_flows_by_name(projection: &Value) -> HashMap<String, RemoteFlow> {
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
