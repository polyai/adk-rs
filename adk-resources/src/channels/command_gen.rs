use crate::channels::local::{ChannelConfiguration, parse_channel_configuration_content};
use crate::push_command;
use crate::push_command_inputs::resource_changed;
use crate::safety_filters::{SafetyFilterMode, parse_safety_filters_content};
use crate::specs::{
    CHAT_CONFIGURATION_FILE, CHAT_SAFETY_FILTERS_FILE, VOICE_CONFIGURATION_FILE,
    VOICE_SAFETY_FILTERS_FILE,
};
use adk_protobuf::Metadata;
use adk_protobuf::channels::channel_update_status::ChannelStatus as ChannelUpdateStatusKind;
use adk_protobuf::channels::{
    ChannelStatus, ChannelType, ChannelUpdateGreeting, ChannelUpdateSafetyFilters,
    ChannelUpdateStatus, ChannelUpdateStylePrompt, VoiceChannelUpdateDisclaimer,
    WebChatChannelUpdateStatus,
};
use adk_protobuf::command::Payload as CommandPayload;
use adk_types::ResourceMap;

pub(crate) fn append_channel_settings_updates(
    commands: &mut Vec<adk_protobuf::Command>,
    resources: &ResourceMap,
    remote_resources: &ResourceMap,
    metadata: &Option<Metadata>,
) {
    if resource_changed(
        resources,
        remote_resources,
        VOICE_SAFETY_FILTERS_FILE.file_path,
    ) && let Some(content) = resource_content(resources, VOICE_SAFETY_FILTERS_FILE.file_path)
    {
        push_channel_safety_filters_update(
            commands,
            metadata,
            ChannelType::Voice,
            VOICE_SAFETY_FILTERS_FILE.file_path,
            content,
        );
    }

    if resource_changed(
        resources,
        remote_resources,
        VOICE_CONFIGURATION_FILE.file_path,
    ) && let Some(config) =
        local_channel_configuration(resources, VOICE_CONFIGURATION_FILE.file_path)
    {
        append_channel_configuration_updates(commands, metadata, ChannelType::Voice, &config);
    }

    let chat_configuration = if resource_changed(
        resources,
        remote_resources,
        CHAT_CONFIGURATION_FILE.file_path,
    ) {
        local_channel_configuration(resources, CHAT_CONFIGURATION_FILE.file_path)
    } else {
        None
    };

    if webchat_config_resource_created(resources, remote_resources) {
        push_webchat_channel_status_update(commands, metadata, true);
    }

    if let Some(config) = chat_configuration.as_ref()
        && let Some(greeting) = config.greeting()
    {
        push_command(
            commands,
            metadata,
            "channel_update_greeting",
            CommandPayload::ChannelUpdateGreeting(ChannelUpdateGreeting {
                channel_type: ChannelType::WebChat as i32,
                greeting: Some(greeting.to_update_proto()),
            }),
        );
    }

    if resource_changed(
        resources,
        remote_resources,
        CHAT_SAFETY_FILTERS_FILE.file_path,
    ) && let Some(content) = resource_content(resources, CHAT_SAFETY_FILTERS_FILE.file_path)
    {
        push_channel_safety_filters_update(
            commands,
            metadata,
            ChannelType::WebChat,
            CHAT_SAFETY_FILTERS_FILE.file_path,
            content,
        );
    }

    if let Some(config) = chat_configuration.as_ref()
        && let Some(style_prompt) = config.style_prompt()
    {
        push_command(
            commands,
            metadata,
            "channel_update_style_prompt",
            CommandPayload::ChannelUpdateStylePrompt(ChannelUpdateStylePrompt {
                channel_type: ChannelType::WebChat as i32,
                style_prompt: Some(style_prompt.to_update_proto()),
            }),
        );
    }
}

fn append_channel_configuration_updates(
    commands: &mut Vec<adk_protobuf::Command>,
    metadata: &Option<Metadata>,
    channel_type: ChannelType,
    config: &ChannelConfiguration,
) {
    if let Some(greeting) = config.greeting() {
        push_command(
            commands,
            metadata,
            "channel_update_greeting",
            CommandPayload::ChannelUpdateGreeting(ChannelUpdateGreeting {
                channel_type: channel_type as i32,
                greeting: Some(greeting.to_update_proto()),
            }),
        );
    }
    if let Some(style_prompt) = config.style_prompt() {
        push_command(
            commands,
            metadata,
            "channel_update_style_prompt",
            CommandPayload::ChannelUpdateStylePrompt(ChannelUpdateStylePrompt {
                channel_type: channel_type as i32,
                style_prompt: Some(style_prompt.to_update_proto()),
            }),
        );
    }
    if matches!(channel_type, ChannelType::Voice)
        && let Some(disclaimer) = config.disclaimer()
    {
        push_command(
            commands,
            metadata,
            "voice_channel_update_disclaimer",
            CommandPayload::VoiceChannelUpdateDisclaimer(VoiceChannelUpdateDisclaimer {
                disclaimer: Some(disclaimer.to_update_proto()),
            }),
        );
    }
}

fn webchat_config_resource_created(
    resources: &ResourceMap,
    remote_resources: &ResourceMap,
) -> bool {
    [
        CHAT_CONFIGURATION_FILE.file_path,
        CHAT_SAFETY_FILTERS_FILE.file_path,
    ]
    .into_iter()
    .any(|path| resources.contains_key(path) && !remote_resources.contains_key(path))
}

fn push_webchat_channel_status_update(
    commands: &mut Vec<adk_protobuf::Command>,
    metadata: &Option<Metadata>,
    enabled: bool,
) {
    let status = if enabled {
        ChannelStatus::Created
    } else {
        ChannelStatus::NotCreated
    };
    push_command(
        commands,
        metadata,
        "channel_update_status",
        CommandPayload::ChannelUpdateStatus(ChannelUpdateStatus {
            channel_status: Some(ChannelUpdateStatusKind::Webchat(
                WebChatChannelUpdateStatus {
                    status: status as i32,
                },
            )),
        }),
    );
}

fn push_channel_safety_filters_update(
    commands: &mut Vec<adk_protobuf::Command>,
    metadata: &Option<Metadata>,
    channel_type: ChannelType,
    path: &str,
    content: &str,
) {
    let Ok(safety_filters) = parse_safety_filters_content(path, content, SafetyFilterMode::Channel)
    else {
        return;
    };
    push_command(
        commands,
        metadata,
        "channel_update_safety_filters",
        CommandPayload::ChannelUpdateSafetyFilters(ChannelUpdateSafetyFilters {
            channel_type: channel_type as i32,
            safety_filters: Some(safety_filters.to_update_proto()),
        }),
    );
}

fn local_channel_configuration(
    resources: &ResourceMap,
    path: &str,
) -> Option<ChannelConfiguration> {
    let content = resource_content(resources, path)?;
    parse_channel_configuration_content(path, content).ok()
}

fn resource_content<'a>(resources: &'a ResourceMap, path: &str) -> Option<&'a str> {
    resources.get(path)?.payload.get("content")?.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::local_resource;

    fn safety_filter_content() -> &'static str {
        r#"
enabled: true
categories:
  violence:
    enabled: true
    level: strict
  hate:
    enabled: false
    level: medium
  sexual:
    enabled: false
    level: lenient
  self_harm:
    enabled: false
    level: medium
"#
    }

    #[test]
    fn voice_configuration_updates_parse_local_content_through_typed_model() {
        let mut resources = ResourceMap::new();
        resources.insert(
            VOICE_CONFIGURATION_FILE.file_path.to_string(),
            local_resource(
                VOICE_CONFIGURATION_FILE.file_path,
                VOICE_CONFIGURATION_FILE.name,
                r#"
greeting:
  welcome_message: Hello
  language_code: en-US
style_prompt:
  prompt: Warm and concise.
disclaimer_messages:
  message: Recorded line
  enabled: true
  language_code: en-US
"#,
            ),
        );
        let mut commands = Vec::new();
        append_channel_settings_updates(&mut commands, &resources, &ResourceMap::new(), &None);

        assert!(commands.iter().any(|command| {
            matches!(
                &command.payload,
                Some(CommandPayload::ChannelUpdateGreeting(ChannelUpdateGreeting {
                    channel_type,
                    greeting: Some(greeting),
                })) if *channel_type == ChannelType::Voice as i32
                    && greeting.welcome_message.as_deref() == Some("Hello")
                    && greeting.language_code == "en-US"
            )
        }));
        assert!(commands.iter().any(|command| {
            matches!(
                &command.payload,
                Some(CommandPayload::ChannelUpdateStylePrompt(ChannelUpdateStylePrompt {
                    channel_type,
                    style_prompt: Some(style_prompt),
                })) if *channel_type == ChannelType::Voice as i32
                    && style_prompt.prompt == "Warm and concise."
            )
        }));
        assert!(commands.iter().any(|command| {
            matches!(
                &command.payload,
                Some(CommandPayload::VoiceChannelUpdateDisclaimer(
                    VoiceChannelUpdateDisclaimer {
                        disclaimer: Some(disclaimer),
                    }
                )) if disclaimer.message.as_deref() == Some("Recorded line")
                    && disclaimer.is_enabled == Some(true)
                    && disclaimer.language_code == "en-US"
            )
        }));
    }

    #[test]
    fn chat_safety_filter_update_parses_local_content_through_typed_model() {
        let mut resources = ResourceMap::new();
        resources.insert(
            CHAT_SAFETY_FILTERS_FILE.file_path.to_string(),
            local_resource(
                CHAT_SAFETY_FILTERS_FILE.file_path,
                CHAT_SAFETY_FILTERS_FILE.name,
                safety_filter_content(),
            ),
        );
        let mut commands = Vec::new();
        append_channel_settings_updates(&mut commands, &resources, &ResourceMap::new(), &None);

        assert!(commands.iter().any(|command| {
            matches!(
                &command.payload,
                Some(CommandPayload::ChannelUpdateStatus(ChannelUpdateStatus {
                    channel_status: Some(ChannelUpdateStatusKind::Webchat(status)),
                })) if status.status == ChannelStatus::Created as i32
            )
        }));
        assert!(commands.iter().any(|command| {
            matches!(
                &command.payload,
                Some(CommandPayload::ChannelUpdateSafetyFilters(ChannelUpdateSafetyFilters {
                    channel_type,
                    safety_filters: Some(filters),
                })) if *channel_type == ChannelType::WebChat as i32
                    && filters.disabled == Some(false)
                    && filters
                        .azure_config
                        .as_ref()
                        .and_then(|config| config.violence.as_ref())
                        .is_some_and(|violence| violence.is_active && violence.precision == "STRICT")
            )
        }));
    }
}
