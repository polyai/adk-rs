use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::flows::{
    ConditionDetails, CreateAdvancedStep, CreateFunctionStep, CreateFunctionStepDefinition,
    CreateNoCodeCondition, CreateNoCodeStep, DeleteNoCodeCondition, DeleteStep, ExitFlowCondition,
    FlowCreateFlow, FlowCreateTransitionFunction, FlowDeleteFlow, FlowDeleteTransitionFunction,
    FlowUpdateFlow, FlowUpdateStep, FlowUpdateStepAsrConfig, FlowUpdateStepDtmfConfig,
    FlowUpdateTransitionFunction, FlowUpdateTransitionFunctionLatencyControl, NoCodeStepReferences,
    StepAsrConfig, StepAsrConfigUpdate, StepDtmfConfig, StepDtmfConfigUpdate, StepPosition,
    TransitionFunctionCreateTransitionFunction, TransitionFunctionReferences,
    TransitionFunctionUpdateTransitionFunction, UpdateAdvancedStep, UpdateNoCodeCondition,
    UpdateNoCodeStep, create_no_code_condition, create_step, update_no_code_condition,
};
use adk_protobuf::functions::{FunctionCreateLatencyControl, FunctionUpdateLatencyControl};
use serde_json::{Value, json};

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
