use crate::CommandGenError;
use crate::materialization::insert_yaml_resource;
use crate::specs::{
    CHAT_CONFIGURATION_FILE, CHAT_SAFETY_FILTERS_FILE, VOICE_CONFIGURATION_FILE,
    VOICE_SAFETY_FILTERS_FILE,
};
use adk_types::ResourceMap;
use serde_json::Value;

pub(crate) fn insert_channel_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    if let Some(voice_safety_filters) = projection.pointer("/channels/voice/config/safetyFilters") {
        insert_yaml_resource(
            map,
            VOICE_SAFETY_FILTERS_FILE.file_path,
            VOICE_SAFETY_FILTERS_FILE.resource_id,
            VOICE_SAFETY_FILTERS_FILE.name,
            safety_filters_yaml(voice_safety_filters, true),
        )?;
    }

    let voice_greeting = projection
        .pointer("/channels/voice/config/greeting")
        .cloned();
    let voice_style_prompt = projection
        .pointer("/channels/voice/config/stylePrompt")
        .cloned();
    let voice_disclaimer = projection.pointer("/channels/voice/disclaimer").cloned();
    if voice_greeting.is_some() || voice_style_prompt.is_some() || voice_disclaimer.is_some() {
        insert_yaml_resource(
            map,
            VOICE_CONFIGURATION_FILE.file_path,
            VOICE_CONFIGURATION_FILE.resource_id,
            VOICE_CONFIGURATION_FILE.name,
            channel_configuration_yaml(
                voice_greeting.as_ref(),
                voice_style_prompt.as_ref(),
                voice_disclaimer.as_ref(),
            ),
        )?;
    }

    if web_chat_channel_is_created(projection.pointer("/channels/webChat")) {
        let chat_greeting = projection
            .pointer("/channels/webChat/config/greeting")
            .cloned();
        let chat_style_prompt = projection
            .pointer("/channels/webChat/config/stylePrompt")
            .cloned();
        if chat_greeting.is_some() || chat_style_prompt.is_some() {
            insert_yaml_resource(
                map,
                CHAT_CONFIGURATION_FILE.file_path,
                CHAT_CONFIGURATION_FILE.resource_id,
                CHAT_CONFIGURATION_FILE.name,
                channel_configuration_yaml(
                    chat_greeting.as_ref(),
                    chat_style_prompt.as_ref(),
                    None,
                ),
            )?;
        }
        if let Some(chat_safety_filters) =
            projection.pointer("/channels/webChat/config/safetyFilters")
        {
            insert_yaml_resource(
                map,
                CHAT_SAFETY_FILTERS_FILE.file_path,
                CHAT_SAFETY_FILTERS_FILE.resource_id,
                CHAT_SAFETY_FILTERS_FILE.name,
                safety_filters_yaml(chat_safety_filters, true),
            )?;
        }
    }

    Ok(())
}

fn web_chat_channel_is_created(channel: Option<&Value>) -> bool {
    let Some(channel) = channel else {
        return false;
    };
    match channel.get("status") {
        Some(Value::Bool(status)) => *status,
        Some(Value::Number(status)) => status.as_i64().is_some_and(|status| status != 0),
        Some(Value::String(status)) => {
            !matches!(status.as_str(), "" | "0" | "NOT_CREATED" | "not_created")
        }
        Some(Value::Null) | None => false,
        Some(_) => true,
    }
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

fn channel_configuration_yaml(
    greeting: Option<&Value>,
    style_prompt: Option<&Value>,
    disclaimer: Option<&Value>,
) -> Value {
    let mut value = serde_json::Map::new();
    value.insert(
        "greeting".to_string(),
        greeting
            .map(channel_greeting_yaml)
            .unwrap_or_else(|| serde_json::json!({})),
    );
    value.insert(
        "style_prompt".to_string(),
        style_prompt
            .map(channel_style_prompt_yaml)
            .unwrap_or_else(|| serde_json::json!({})),
    );
    if let Some(disclaimer) = disclaimer {
        value.insert(
            "disclaimer_messages".to_string(),
            channel_disclaimer_yaml(disclaimer),
        );
    }
    Value::Object(value)
}

fn channel_greeting_yaml(greeting: &Value) -> Value {
    serde_json::json!({
        "welcome_message": greeting
            .get("welcomeMessage")
            .or_else(|| greeting.get("welcome_message"))
            .and_then(Value::as_str)
            .unwrap_or_default(),
        "language_code": greeting
            .get("languageCode")
            .or_else(|| greeting.get("language_code"))
            .and_then(Value::as_str)
            .unwrap_or("en-GB"),
    })
}

fn channel_style_prompt_yaml(style_prompt: &Value) -> Value {
    serde_json::json!({
        "prompt": style_prompt
            .get("prompt")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    })
}

fn channel_disclaimer_yaml(disclaimer: &Value) -> Value {
    serde_json::json!({
        "message": disclaimer
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        "enabled": disclaimer
            .get("isEnabled")
            .or_else(|| disclaimer.get("enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "language_code": disclaimer
            .get("languageCode")
            .or_else(|| disclaimer.get("language_code"))
            .and_then(Value::as_str)
            .unwrap_or("en-GB"),
    })
}

#[cfg(test)]
mod tests {
    use crate::projection_to_resource_map;

    #[test]
    fn projection_materializes_safety_filters_as_python_yaml_shape() {
        let projection = serde_json::json!({
            "contentFilterSettings": {
                "disabled": true,
                "type": "azure",
                "azureConfig": {
                    "violence": {"isActive": true, "precision": "STRICT"},
                    "hate": {"isActive": false, "precision": "MEDIUM"},
                    "sexual": {"isActive": false, "precision": "LOOSE"},
                    "selfHarm": {"isActive": true, "precision": "STRICT"}
                }
            },
            "channels": {
                "voice": {
                    "config": {
                        "safetyFilters": {
                            "disabled": false,
                            "azureConfig": {
                                "violence": {"isActive": true, "precision": "STRICT"},
                                "hate": {"isActive": true, "precision": "MEDIUM"},
                                "sexual": {"isActive": false, "precision": "LOOSE"},
                                "selfHarm": {"isActive": false, "precision": "MEDIUM"}
                            }
                        }
                    }
                }
            }
        });

        let resources = projection_to_resource_map(&projection).expect("projection resources");
        let general = resources
            .get("agent_settings/safety_filters.yaml")
            .and_then(|resource| resource.payload.get("content"))
            .and_then(serde_json::Value::as_str)
            .expect("general safety filter YAML");
        let voice = resources
            .get("voice/safety_filters.yaml")
            .and_then(|resource| resource.payload.get("content"))
            .and_then(serde_json::Value::as_str)
            .expect("voice safety filter YAML");

        assert!(!general.contains("azureConfig"));
        assert!(!general.contains("disabled:"));
        assert!(general.contains("categories:"));
        assert!(general.contains("self_harm:"));
        assert!(general.contains("level: strict"));
        assert!(voice.contains("enabled: true"));
    }

    #[test]
    fn projection_materializes_channel_configuration_as_python_yaml_shape() {
        let projection = serde_json::json!({
            "channels": {
                "voice": {
                    "config": {
                        "greeting": {
                            "welcomeMessage": "Hello",
                            "languageCode": "en-US"
                        },
                        "stylePrompt": {"prompt": "Warm and concise"}
                    },
                    "disclaimer": {
                        "message": "Recorded line",
                        "isEnabled": true,
                        "languageCode": "en-US"
                    }
                },
                "webChat": {
                    "status": true,
                    "config": {
                        "greeting": {
                            "welcomeMessage": "Hi",
                            "languageCode": "en-GB"
                        },
                        "stylePrompt": {"prompt": "Helpful"}
                    }
                }
            }
        });

        let resources = projection_to_resource_map(&projection).expect("projection resources");
        let voice = resources
            .get("voice/configuration.yaml")
            .and_then(|resource| resource.payload.get("content"))
            .and_then(serde_json::Value::as_str)
            .expect("voice configuration YAML");
        let chat = resources
            .get("chat/configuration.yaml")
            .and_then(|resource| resource.payload.get("content"))
            .and_then(serde_json::Value::as_str)
            .expect("chat configuration YAML");

        assert!(voice.contains("welcome_message: Hello"));
        assert!(voice.contains("language_code: en-US"));
        assert!(voice.contains("disclaimer_messages:"));
        assert!(voice.contains("enabled: true"));
        assert!(!voice.contains("welcomeMessage"));
        assert!(!voice.contains("- message:"));
        assert!(chat.contains("welcome_message: Hi"));
        assert!(chat.contains("style_prompt:"));
    }
}
