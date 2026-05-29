use crate::CommandGenError;
use crate::materialization::insert_yaml_resource;
use crate::specs::ASR_SETTINGS_FILE;
use adk_types::ResourceMap;
use serde_json::Value;

pub(crate) fn insert_asr_settings_resource(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    if let Some(asr_settings) = projection
        .pointer("/channels/voice/asrSettings")
        .or_else(|| projection.get("asrSettings"))
    {
        insert_yaml_resource(
            map,
            ASR_SETTINGS_FILE.file_path,
            ASR_SETTINGS_FILE.resource_id,
            ASR_SETTINGS_FILE.name,
            asr_settings_yaml(asr_settings),
        )?;
    }

    Ok(())
}

fn asr_settings_yaml(settings: &Value) -> Value {
    let latency_config = settings
        .get("latencyConfig")
        .or_else(|| settings.get("latency_config"));
    serde_json::json!({
        "barge_in": settings
            .get("bargeIn")
            .or_else(|| settings.get("barge_in"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "interaction_style": latency_config
            .and_then(|config| {
                config
                    .get("interactionStyle")
                    .or_else(|| config.get("interaction_style"))
            })
            .or_else(|| {
                settings
                    .get("interactionStyle")
                    .or_else(|| settings.get("interaction_style"))
            })
            .and_then(Value::as_str)
            .unwrap_or("balanced"),
    })
}

#[cfg(test)]
mod tests {
    use crate::projection_to_resource_map;

    #[test]
    fn projection_materializes_asr_settings_as_python_yaml_shape() {
        let projection = serde_json::json!({
            "channels": {
                "voice": {
                    "asrSettings": {
                        "bargeIn": false,
                        "latencyConfig": {
                            "interactionStyle": "precise"
                        },
                        "updatedAt": "2026-01-21T14:35:16.078Z",
                        "updatedBy": "miles.nash@poly-ai.com"
                    }
                }
            }
        });

        let resources = projection_to_resource_map(&projection).expect("projection resources");
        let content = resources
            .get("voice/speech_recognition/asr_settings.yaml")
            .and_then(|resource| resource.payload.get("content"))
            .and_then(serde_json::Value::as_str)
            .expect("ASR settings YAML");

        assert!(content.contains("barge_in: false"));
        assert!(content.contains("interaction_style: precise"));
        assert!(!content.contains("bargeIn"));
        assert!(!content.contains("latencyConfig"));
        assert!(!content.contains("updatedAt"));
        assert!(!content.contains("updatedBy"));
    }
}
