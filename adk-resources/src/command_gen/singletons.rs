//! Push commands for singleton files.
//!
//! Singleton files represent one backend or configuration resource with their
//! own command semantics, such as role, personality, rules, channel settings,
//! ASR settings, safety filters, and experimental config.

use super::CommandGroups;
use crate::projection_to_resource_map;
use adk_protobuf::Metadata;
use adk_protobuf::command::Payload as CommandPayload;
use adk_types::ResourceMap;
use serde_json::Value;

pub(crate) fn singleton_command_groups(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> CommandGroups {
    let remote_resources = projection_to_resource_map(projection).unwrap_or_default();
    let mut groups = CommandGroups::default();

    crate::agent_settings::append_agent_settings_updates(
        &mut groups.updates,
        resources,
        &remote_resources,
        projection,
        metadata,
    );
    crate::experimental_config::append_experimental_config_update(
        &mut groups.updates,
        resources,
        projection,
        metadata,
    );
    crate::channels::append_channel_settings_updates(
        &mut groups.updates,
        resources,
        &remote_resources,
        metadata,
    );
    crate::asr_settings::append_asr_settings_update(
        &mut groups.updates,
        resources,
        &remote_resources,
        metadata,
    );

    groups
}

pub(crate) fn payload_json_summary(payload: &CommandPayload) -> Option<(&'static str, Value)> {
    crate::agent_settings::payload_json_summary(payload)
        .or_else(|| crate::experimental_config::payload_json_summary(payload))
        .or_else(|| crate::asr_settings::payload_json_summary(payload))
        .or_else(|| crate::channels::payload_json_summary(payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::local_resource;
    use adk_protobuf::channels::ChannelType;

    fn flatten(groups: CommandGroups) -> Vec<adk_protobuf::Command> {
        groups
            .deletes
            .into_iter()
            .chain(groups.creates)
            .chain(groups.updates)
            .chain(groups.post_updates)
            .collect()
    }

    #[test]
    fn singleton_files_emit_real_update_commands() {
        let mut resources = ResourceMap::new();
        resources.insert(
            "agent_settings/personality.yaml".to_string(),
            local_resource(
                "agent_settings/personality.yaml",
                "personality",
                "adjectives:\n  Curious: true\ncustom: Recording parity custom personality.\n",
            ),
        );
        resources.insert(
            "agent_settings/role.yaml".to_string(),
            local_resource(
                "agent_settings/role.yaml",
                "role",
                "value: CustomerServiceRepresentative\nadditional_info: Recording parity role detail.\ncustom: ''\n",
            ),
        );
        resources.insert(
            "agent_settings/safety_filters.yaml".to_string(),
            local_resource(
                "agent_settings/safety_filters.yaml",
                "safety_filters",
                "enabled: true\ncategories:\n  violence:\n    enabled: true\n    level: medium\n",
            ),
        );
        resources.insert(
            "voice/configuration.yaml".to_string(),
            local_resource(
                "voice/configuration.yaml",
                "voice_configuration",
                r#"
greeting:
  welcome_message: Hello from tests.
  language_code: en-US
style_prompt:
  prompt: Keep it compact.
disclaimer_messages:
  enabled: true
  message: This call may be recorded.
  language_code: en-US
"#,
            ),
        );
        resources.insert(
            "voice/speech_recognition/asr_settings.yaml".to_string(),
            local_resource(
                "voice/speech_recognition/asr_settings.yaml",
                "asr_settings",
                "barge_in: true\ninteraction_style: balanced\n",
            ),
        );
        resources.insert(
            "voice/safety_filters.yaml".to_string(),
            local_resource(
                "voice/safety_filters.yaml",
                "voice_safety_filters",
                "enabled: true\ncategories:\n  violence:\n    enabled: true\n    level: medium\n",
            ),
        );
        resources.insert(
            "chat/configuration.yaml".to_string(),
            local_resource(
                "chat/configuration.yaml",
                "chat_configuration",
                r#"
greeting:
  welcome_message: Hello from chat.
  language_code: en-US
style_prompt:
  prompt: Keep webchat compact.
"#,
            ),
        );
        resources.insert(
            "chat/safety_filters.yaml".to_string(),
            local_resource(
                "chat/safety_filters.yaml",
                "chat_safety_filters",
                "enabled: true\ncategories:\n  hate:\n    enabled: true\n    level: medium\n",
            ),
        );

        let commands = flatten(singleton_command_groups(
            &resources,
            &serde_json::json!({}),
            &None,
        ));
        let types = commands
            .iter()
            .map(|command| command.r#type.as_str())
            .collect::<Vec<_>>();
        for expected in [
            "update_personality",
            "update_role",
            "update_content_filter_settings",
            "channel_update_greeting",
            "channel_update_style_prompt",
            "channel_update_safety_filters",
            "voice_channel_update_disclaimer",
            "voice_channel_update_asr_settings",
        ] {
            assert!(
                types.contains(&expected),
                "missing singleton update command: {expected}"
            );
        }

        let asr = commands
            .iter()
            .find(|command| command.r#type == "voice_channel_update_asr_settings")
            .expect("voice_channel_update_asr_settings command");
        match &asr.payload {
            Some(CommandPayload::VoiceChannelUpdateAsrSettings(payload)) => {
                let settings = payload.asr_settings.as_ref().expect("asr settings");
                assert_eq!(settings.barge_in, Some(true));
                assert_eq!(
                    settings
                        .latency_config
                        .as_ref()
                        .map(|config| config.interaction_style.as_str()),
                    Some("balanced")
                );
            }
            _ => panic!("unexpected payload variant for voice_channel_update_asr_settings command"),
        }

        let webchat_greeting = commands
            .iter()
            .find(|command| {
                command.r#type == "channel_update_greeting"
                    && matches!(
                        command.payload.as_ref(),
                        Some(CommandPayload::ChannelUpdateGreeting(payload))
                            if payload.channel_type == ChannelType::WebChat as i32
                    )
            })
            .expect("webchat greeting update");
        match &webchat_greeting.payload {
            Some(CommandPayload::ChannelUpdateGreeting(payload)) => {
                assert_eq!(payload.channel_type, ChannelType::WebChat as i32);
                assert_eq!(
                    payload
                        .greeting
                        .as_ref()
                        .and_then(|greeting| greeting.welcome_message.as_deref()),
                    Some("Hello from chat.")
                );
            }
            _ => panic!("unexpected payload variant for webchat greeting command"),
        }

        assert!(commands.iter().any(|command| {
            command.r#type == "channel_update_safety_filters"
                && matches!(
                    command.payload.as_ref(),
                    Some(CommandPayload::ChannelUpdateSafetyFilters(payload))
                        if payload.channel_type == ChannelType::Voice as i32
                )
        }));
        assert!(commands.iter().any(|command| {
            command.r#type == "channel_update_safety_filters"
                && matches!(
                    command.payload.as_ref(),
                    Some(CommandPayload::ChannelUpdateSafetyFilters(payload))
                        if payload.channel_type == ChannelType::WebChat as i32
                )
        }));
    }
}
