use crate::CommandGenError;
use crate::materialization::{insert_content_resource, insert_yaml_resource};
use crate::specs::{
    AGENT_PERSONALITY_FILE, AGENT_ROLE_FILE, AGENT_RULES_FILE, AGENT_SAFETY_FILTERS_FILE,
};
use adk_types::ResourceMap;
use serde_json::Value;

pub(crate) fn insert_profile_and_safety_resources(
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

pub(crate) fn insert_rules_resource(
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

fn safety_filters_yaml(settings: &Value, include_enabled: bool) -> Value {
    let azure_config = settings
        .get("azureConfig")
        .or_else(|| settings.get("azure_config"))
        .unwrap_or(&Value::Null);
    let mut categories = serde_json::Map::new();
    for (yaml_key, backend_keys) in [
        ("violence", ["violence", "violence"]),
        ("hate", ["hate", "hate"]),
        ("sexual", ["sexual", "sexual"]),
        ("self_harm", ["selfHarm", "self_harm"]),
    ] {
        let category = backend_keys
            .iter()
            .find_map(|key| azure_config.get(*key))
            .map(safety_filter_category_yaml)
            .unwrap_or_else(|| serde_json::json!({}));
        categories.insert(yaml_key.to_string(), category);
    }

    let mut value = serde_json::Map::new();
    if include_enabled {
        value.insert(
            "enabled".to_string(),
            Value::Bool(
                !settings
                    .get("disabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            ),
        );
    }
    value.insert("categories".to_string(), Value::Object(categories));
    Value::Object(value)
}

fn safety_filter_category_yaml(category: &Value) -> Value {
    serde_json::json!({
        "enabled": category
            .get("isActive")
            .or_else(|| category.get("is_active"))
            .and_then(Value::as_bool),
        "level": safety_filter_precision_level(
            category
                .get("precision")
                .and_then(Value::as_str)
                .unwrap_or_default()
        ),
    })
}

fn safety_filter_precision_level(precision: &str) -> String {
    match precision {
        "LOOSE" => "lenient".to_string(),
        "MEDIUM" => "medium".to_string(),
        "STRICT" => "strict".to_string(),
        value => value.to_ascii_lowercase(),
    }
}

#[cfg(test)]
mod tests {
    use crate::projection_to_resource_map;

    #[test]
    fn projection_materializes_agent_settings_as_python_yaml_shape() {
        let projection = serde_json::json!({
            "agentSettings": {
                "personality": {
                    "adjectives": {"values": {"Calm": true}},
                    "custom": "Be helpful",
                    "createdAt": "ignored"
                },
                "role": {
                    "value": "other",
                    "additionalInfo": "Receptionist",
                    "custom": "Custom role",
                    "updatedAt": "ignored"
                }
            }
        });

        let resources = projection_to_resource_map(&projection).expect("projection resources");
        let personality = resources
            .get("agent_settings/personality.yaml")
            .and_then(|resource| resource.payload.get("content"))
            .and_then(serde_json::Value::as_str)
            .expect("personality YAML");
        let role = resources
            .get("agent_settings/role.yaml")
            .and_then(|resource| resource.payload.get("content"))
            .and_then(serde_json::Value::as_str)
            .expect("role YAML");

        assert!(personality.contains("adjectives:"));
        assert!(personality.contains("custom: Be helpful"));
        assert!(!personality.contains("createdAt"));
        assert!(role.contains("additional_info: Receptionist"));
        assert!(!role.contains("additionalInfo"));
        assert!(!role.contains("updatedAt"));
    }
}
