use crate::asr_settings::local::parse_asr_settings_content;
use crate::push_command;
use crate::push_command_inputs::resource_changed;
use crate::specs::ASR_SETTINGS_FILE;
use adk_protobuf::Metadata;
use adk_protobuf::asr_settings::AsrSettingsUpdateAsrSettings;
use adk_protobuf::channels::VoiceChannelUpdateAsrSettings;
use adk_protobuf::command::Payload as CommandPayload;
use adk_types::ResourceMap;
use serde_json::{self, Value as JsonValue, json};

pub(crate) fn append_asr_settings_update(
    commands: &mut Vec<adk_protobuf::Command>,
    resources: &ResourceMap,
    remote_resources: &ResourceMap,
    metadata: &Option<Metadata>,
) {
    if resource_changed(resources, remote_resources, ASR_SETTINGS_FILE.file_path)
        && let Some(content) = resource_content(resources, ASR_SETTINGS_FILE.file_path)
        && let Ok(asr_settings) = parse_asr_settings_content(ASR_SETTINGS_FILE.file_path, content)
    {
        push_command(
            commands,
            metadata,
            "voice_channel_update_asr_settings",
            CommandPayload::VoiceChannelUpdateAsrSettings(VoiceChannelUpdateAsrSettings {
                asr_settings: Some(asr_settings.to_update_proto()),
            }),
        );
    }
}

fn resource_content<'a>(resources: &'a ResourceMap, path: &str) -> Option<&'a str> {
    resources.get(path)?.payload.get("content")?.as_str()
}

pub(crate) fn payload_json_summary(payload: &CommandPayload) -> Option<(&'static str, JsonValue)> {
    match payload {
        CommandPayload::VoiceChannelUpdateAsrSettings(msg) => Some((
            "voice_channel_update_asr_settings",
            json!({
                "asr_settings": msg
                    .asr_settings
                    .as_ref()
                    .map(asr_settings_json)
                    .unwrap_or_else(|| json!({})),
            }),
        )),
        _ => None,
    }
}

fn asr_settings_json(settings: &AsrSettingsUpdateAsrSettings) -> JsonValue {
    json!({
        "barge_in": settings.barge_in.unwrap_or(false),
        "latency_config": {
            "interaction_style": settings
                .latency_config
                .as_ref()
                .map(|config| config.interaction_style.clone())
                .unwrap_or_default(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::local_resource;

    #[test]
    fn asr_update_parses_local_content_through_typed_model() {
        let mut resources = ResourceMap::new();
        resources.insert(
            ASR_SETTINGS_FILE.file_path.to_string(),
            local_resource(
                ASR_SETTINGS_FILE.file_path,
                ASR_SETTINGS_FILE.name,
                "barge_in: true\ninteraction_style: turbo\n",
            ),
        );
        let mut commands = Vec::new();
        append_asr_settings_update(&mut commands, &resources, &ResourceMap::new(), &None);

        let command = commands
            .iter()
            .find(|command| command.r#type == "voice_channel_update_asr_settings")
            .expect("voice_channel_update_asr_settings command");
        match &command.payload {
            Some(CommandPayload::VoiceChannelUpdateAsrSettings(payload)) => {
                let settings = payload.asr_settings.as_ref().expect("ASR settings");
                assert_eq!(settings.barge_in, Some(true));
                assert_eq!(
                    settings
                        .latency_config
                        .as_ref()
                        .map(|config| config.interaction_style.as_str()),
                    Some("sonic")
                );
            }
            _ => panic!("unexpected voice_channel_update_asr_settings payload"),
        }
    }
}
