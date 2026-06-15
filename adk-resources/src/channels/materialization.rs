use crate::CommandGenError;
use crate::channels::local::ChannelConfiguration;
use crate::materialization::insert_yaml_resource;
use crate::safety_filters::SafetyFilters;
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
            SafetyFilters::from_projection(voice_safety_filters, true),
        )?;
    }

    let voice_greeting = projection
        .pointer("/channels/voice/config/greeting")
        .cloned();
    let voice_style_prompt = projection
        .pointer("/channels/voice/config/stylePrompt")
        .cloned();
    let voice_disclaimer = projection.pointer("/channels/voice/disclaimer").cloned();
    let voice_configuration = ChannelConfiguration::from_projection(
        voice_greeting.as_ref(),
        voice_style_prompt.as_ref(),
        voice_disclaimer.as_ref(),
    );
    if !voice_configuration.is_empty() {
        insert_yaml_resource(
            map,
            VOICE_CONFIGURATION_FILE.file_path,
            VOICE_CONFIGURATION_FILE.resource_id,
            VOICE_CONFIGURATION_FILE.name,
            voice_configuration,
        )?;
    }

    if web_chat_channel_is_created(projection.pointer("/channels/webChat")) {
        let chat_greeting = projection
            .pointer("/channels/webChat/config/greeting")
            .cloned();
        let chat_style_prompt = projection
            .pointer("/channels/webChat/config/stylePrompt")
            .cloned();
        let chat_configuration = ChannelConfiguration::from_projection(
            chat_greeting.as_ref(),
            chat_style_prompt.as_ref(),
            None,
        );
        if !chat_configuration.is_empty() {
            insert_yaml_resource(
                map,
                CHAT_CONFIGURATION_FILE.file_path,
                CHAT_CONFIGURATION_FILE.resource_id,
                CHAT_CONFIGURATION_FILE.name,
                chat_configuration,
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
                SafetyFilters::from_projection(chat_safety_filters, true),
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

    #[test]
    fn projection_omits_empty_channel_greeting_when_other_sections_exist() {
        let projection = serde_json::json!({
            "channels": {
                "webChat": {
                    "status": true,
                    "config": {
                        "greeting": {},
                        "stylePrompt": {"prompt": "Helpful"}
                    }
                }
            }
        });

        let resources = projection_to_resource_map(&projection).expect("projection resources");
        let chat = resources
            .get("chat/configuration.yaml")
            .and_then(|resource| resource.payload.get("content"))
            .and_then(serde_json::Value::as_str)
            .expect("chat configuration YAML");

        assert!(!chat.contains("greeting:"));
        assert!(chat.contains("style_prompt:"));
        assert!(chat.contains("prompt: Helpful"));
    }
}
