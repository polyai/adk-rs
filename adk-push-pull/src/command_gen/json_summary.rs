use super::{flows, single_file_resources};
use adk_protobuf::agent::{RulesReferences, RulesUpdateRules};
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::functions::{FunctionParameter, FunctionUpdateLatencyControl};
use adk_protobuf::knowledge_base::{KnowledgeBaseCreateTopic, TopicReferences};
use adk_protobuf::variables::VariableReferences;
use adk_protobuf::{Command, Metadata};
use serde_json::Value;
use std::collections::HashMap;

pub fn command_to_json_summary(command: &Command) -> Value {
    let mut value = serde_json::json!({
        "type": command.r#type,
        "command_id": command.command_id,
    });
    if let Some(metadata) = &command.metadata {
        value["metadata"] = metadata_to_json(metadata);
    }
    if let Some(payload) = &command.payload {
        match payload {
            CommandPayload::DeleteFunction(delete) => {
                value["delete_function"] = serde_json::json!({ "id": delete.id });
            }
            CommandPayload::DeleteStartFunction(delete) => {
                value["delete_start_function"] = serde_json::json!({ "id": delete.id });
            }
            CommandPayload::DeleteEndFunction(delete) => {
                value["delete_end_function"] = serde_json::json!({ "id": delete.id });
            }
            CommandPayload::VariableDelete(delete) => {
                value["variable_delete"] = serde_json::json!({ "id": delete.id });
            }
            CommandPayload::VariableCreate(create) => {
                value["variable_create"] =
                    variable_command_to_json(&create.id, &create.name, create.references.as_ref());
            }
            CommandPayload::VariableUpdate(update) => {
                value["variable_update"] =
                    variable_command_to_json(&update.id, &update.name, update.references.as_ref());
            }
            CommandPayload::CreateStartFunction(create) => {
                value["create_start_function"] = special_function_create_to_json(
                    &create.id,
                    &create.name,
                    &create.description,
                    &create.parameters,
                    &create.code,
                    create.archived,
                    create.references.as_ref().map(|refs| &refs.variables),
                );
            }
            CommandPayload::CreateEndFunction(create) => {
                value["create_end_function"] = special_function_create_to_json(
                    &create.id,
                    &create.name,
                    &create.description,
                    &create.parameters,
                    &create.code,
                    create.archived,
                    create.references.as_ref().map(|refs| &refs.variables),
                );
            }
            CommandPayload::UpdateStartFunction(update) => {
                value["update_start_function"] = special_function_update_to_json(
                    &update.id,
                    update.description.as_deref(),
                    update.code.as_deref(),
                    update.references.as_ref().map(|refs| &refs.variables),
                );
            }
            CommandPayload::UpdateEndFunction(update) => {
                value["update_end_function"] = special_function_update_to_json(
                    &update.id,
                    update.description.as_deref(),
                    update.code.as_deref(),
                    update.references.as_ref().map(|refs| &refs.variables),
                );
            }
            CommandPayload::UpdateLatencyControl(update) => {
                value["update_latency_control"] = function_update_latency_control_to_json(update);
            }
            CommandPayload::CreateTopic(topic) => {
                value["create_topic"] = create_topic_to_json(topic);
            }
            CommandPayload::UpdateRules(update) => {
                value["update_rules"] = rules_update_to_json(update);
            }
            _ => {}
        }
        if let Some((key, payload_value)) = single_file_resources::payload_json_summary(payload) {
            value[key] = payload_value;
        }
        if let Some((key, payload_value)) = flows::payload_json_summary(payload) {
            value[key] = payload_value;
        }
    }
    value
}

fn metadata_to_json(metadata: &Metadata) -> Value {
    let created_at = metadata
        .created_at
        .as_ref()
        .map(|timestamp| format!("{}.{:09}Z", timestamp.seconds, timestamp.nanos))
        .unwrap_or_default();
    serde_json::json!({
        "created_at": created_at,
        "created_by": metadata.created_by,
    })
}

fn rules_update_to_json(update: &RulesUpdateRules) -> Value {
    let mut value = serde_json::Map::new();
    if let Some(behaviour) = &update.behaviour {
        value.insert("behaviour".to_string(), Value::String(behaviour.clone()));
    }
    if let Some(references) = &update.references {
        let references_json = rules_references_to_json(references);
        if references_json
            .as_object()
            .map(|object| !object.is_empty())
            .unwrap_or(false)
        {
            value.insert("references".to_string(), references_json);
        }
    }
    Value::Object(value)
}

fn special_function_create_to_json(
    id: &str,
    name: &str,
    description: &str,
    parameters: &[FunctionParameter],
    code: &str,
    archived: Option<bool>,
    variables: Option<&HashMap<String, bool>>,
) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("id".to_string(), Value::String(id.to_string()));
    value.insert("name".to_string(), Value::String(name.to_string()));
    if !description.is_empty() {
        value.insert(
            "description".to_string(),
            Value::String(description.to_string()),
        );
    }
    if !parameters.is_empty() {
        value.insert(
            "parameters".to_string(),
            Value::Array(
                parameters
                    .iter()
                    .map(function_parameter_to_json)
                    .collect::<Vec<_>>(),
            ),
        );
    }
    value.insert("code".to_string(), Value::String(code.to_string()));
    if archived == Some(true) {
        value.insert("archived".to_string(), Value::Bool(true));
    }
    let references = special_function_references_to_json(variables);
    if references
        .as_object()
        .is_some_and(|object| !object.is_empty())
    {
        value.insert("references".to_string(), references);
    }
    Value::Object(value)
}

fn special_function_update_to_json(
    id: &str,
    description: Option<&str>,
    code: Option<&str>,
    variables: Option<&HashMap<String, bool>>,
) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("id".to_string(), Value::String(id.to_string()));
    if let Some(description) = description {
        value.insert(
            "description".to_string(),
            Value::String(description.to_string()),
        );
    }
    if let Some(code) = code {
        value.insert("code".to_string(), Value::String(code.to_string()));
    }
    let references = special_function_references_to_json(variables);
    if references
        .as_object()
        .is_some_and(|object| !object.is_empty())
    {
        value.insert("references".to_string(), references);
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
                        serde_json::json!({
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

fn variable_command_to_json(
    id: &str,
    name: &str,
    references: Option<&VariableReferences>,
) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("id".to_string(), Value::String(id.to_string()));
    value.insert("name".to_string(), Value::String(name.to_string()));
    if let Some(references) = references {
        let references = variable_references_to_json(references);
        if references
            .as_object()
            .is_some_and(|object| !object.is_empty())
        {
            value.insert("references".to_string(), references);
        }
    }
    Value::Object(value)
}

fn variable_references_to_json(references: &VariableReferences) -> Value {
    let mut value = serde_json::Map::new();
    insert_bool_map(&mut value, "functions", &references.functions);
    insert_bool_map(&mut value, "delay_responses", &references.delay_responses);
    insert_bool_map(&mut value, "flow_steps", &references.flow_steps);
    insert_bool_map(
        &mut value,
        "flow_no_code_steps",
        &references.flow_no_code_steps,
    );
    insert_bool_map(&mut value, "flow_functions", &references.flow_functions);
    insert_bool_map(&mut value, "topics", &references.topics);
    insert_bool_map(&mut value, "behaviours", &references.behaviours);
    insert_bool_map(&mut value, "greetings", &references.greetings);
    insert_bool_map(&mut value, "roles", &references.roles);
    insert_bool_map(&mut value, "personalities", &references.personalities);
    insert_bool_map(&mut value, "sms", &references.sms);
    insert_bool_map(&mut value, "start_functions", &references.start_functions);
    insert_bool_map(&mut value, "end_functions", &references.end_functions);
    Value::Object(value)
}

fn function_parameter_to_json(parameter: &FunctionParameter) -> Value {
    serde_json::json!({
        "id": parameter.id.clone(),
        "name": parameter.name.clone(),
        "description": parameter.description.clone(),
        "type": parameter.r#type.clone(),
    })
}

fn special_function_references_to_json(variables: Option<&HashMap<String, bool>>) -> Value {
    let mut value = serde_json::Map::new();
    if let Some(variables) = variables {
        insert_bool_map(&mut value, "variables", variables);
    }
    Value::Object(value)
}

fn create_topic_to_json(topic: &KnowledgeBaseCreateTopic) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("id".to_string(), Value::String(topic.id.clone()));
    value.insert("name".to_string(), Value::String(topic.name.clone()));
    value.insert("content".to_string(), Value::String(topic.content.clone()));
    value.insert("actions".to_string(), Value::String(topic.actions.clone()));
    if let Some(example_queries) = &topic.example_queries {
        value.insert(
            "example_queries".to_string(),
            serde_json::json!({ "queries": example_queries.queries.clone() }),
        );
    }
    value.insert(
        "references".to_string(),
        topic
            .references
            .as_ref()
            .map(topic_references_to_json)
            .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
    );
    if let Some(is_active) = topic.is_active {
        value.insert("is_active".to_string(), Value::Bool(is_active));
    }
    Value::Object(value)
}

fn topic_references_to_json(references: &TopicReferences) -> Value {
    let mut value = serde_json::Map::new();
    insert_bool_map(&mut value, "sms", &references.sms);
    insert_bool_map(&mut value, "handoff", &references.handoff);
    insert_bool_map(&mut value, "attributes", &references.attributes);
    insert_bool_map(&mut value, "globalFunctions", &references.global_functions);
    insert_bool_map(&mut value, "variables", &references.variables);
    insert_bool_map(&mut value, "translations", &references.translations);
    Value::Object(value)
}

fn rules_references_to_json(references: &RulesReferences) -> Value {
    let mut value = serde_json::Map::new();
    insert_bool_map(&mut value, "sms", &references.sms);
    insert_bool_map(&mut value, "handoff", &references.handoff);
    insert_bool_map(&mut value, "attributes", &references.attributes);
    insert_bool_map(&mut value, "globalFunctions", &references.global_functions);
    insert_bool_map(&mut value, "variables", &references.variables);
    insert_bool_map(&mut value, "translations", &references.translations);
    Value::Object(value)
}

fn insert_bool_map(
    target: &mut serde_json::Map<String, Value>,
    key: &str,
    source: &HashMap<String, bool>,
) {
    if source.is_empty() {
        return;
    }
    target.insert(key.to_string(), serde_json::json!(source));
}
