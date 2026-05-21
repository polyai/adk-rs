use super::{channels::safety_filters_yaml, insert_content_resource, insert_yaml_resource};
use crate::CommandGenError;
use crate::resource_specs::{
    AGENT_PERSONALITY_FILE, AGENT_ROLE_FILE, AGENT_RULES_FILE, AGENT_SAFETY_FILTERS_FILE,
};
use adk_types::ResourceMap;
use serde_json::Value;

pub(super) fn insert_profile_and_safety_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    if let Some(personality) = projection.pointer("/agentSettings/personality") {
        insert_yaml_resource(
            map,
            AGENT_PERSONALITY_FILE.file_path,
            AGENT_PERSONALITY_FILE.resource_id,
            AGENT_PERSONALITY_FILE.name,
            personality_yaml(personality),
        )?;
    }

    if let Some(role) = projection.pointer("/agentSettings/role") {
        insert_yaml_resource(
            map,
            AGENT_ROLE_FILE.file_path,
            AGENT_ROLE_FILE.resource_id,
            AGENT_ROLE_FILE.name,
            role_yaml(role),
        )?;
    }

    if let Some(safety_filters) = projection.get("contentFilterSettings") {
        insert_yaml_resource(
            map,
            AGENT_SAFETY_FILTERS_FILE.file_path,
            AGENT_SAFETY_FILTERS_FILE.resource_id,
            AGENT_SAFETY_FILTERS_FILE.name,
            safety_filters_yaml(safety_filters, false),
        )?;
    }

    Ok(())
}

pub(super) fn insert_rules_resource(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    if let Some(behaviour) = projection
        .pointer("/agentSettings/rules/behaviour")
        .and_then(Value::as_str)
    {
        insert_content_resource(
            map,
            AGENT_RULES_FILE.file_path,
            AGENT_RULES_FILE.resource_id,
            AGENT_RULES_FILE.name,
            behaviour.to_string(),
        )?;
    }

    Ok(())
}

fn personality_yaml(personality: &Value) -> Value {
    let adjectives = personality
        .pointer("/adjectives/values")
        .or_else(|| personality.get("adjectives"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    serde_json::json!({
        "adjectives": adjectives,
        "custom": personality
            .get("custom")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    })
}

fn role_yaml(role: &Value) -> Value {
    serde_json::json!({
        "value": role
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        "additional_info": role
            .get("additionalInfo")
            .or_else(|| role.get("additional_info"))
            .and_then(Value::as_str)
            .unwrap_or_default(),
        "custom": role
            .get("custom")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    })
}
