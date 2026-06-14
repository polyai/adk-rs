use crate::asr_settings::local::parse_asr_settings;
use crate::push_command;
use crate::push_command_inputs::{resource_changed, resource_yaml};
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
        && let Some(yaml) = resource_yaml(resources, ASR_SETTINGS_FILE.file_path)
        && let Ok(asr_settings) = parse_asr_settings(ASR_SETTINGS_FILE.file_path, &yaml)
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
