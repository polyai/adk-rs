use crate::channels::{ChatSafetyFilters, VoiceSafetyFilters};
use crate::local_parse::ParseLocalResource;
use crate::push_command_inputs::{first_yaml_mapping, resource_changed, resource_yaml};
use crate::specs::{
    CHAT_CONFIGURATION_FILE, CHAT_SAFETY_FILTERS_FILE, VOICE_CONFIGURATION_FILE,
    VOICE_SAFETY_FILTERS_FILE,
};
use crate::{push_command, yaml_str};
use adk_protobuf::Metadata;
use adk_protobuf::agent::{DisclaimerMessageUpdateDisclaimerMessage, GreetingUpdateGreeting};
use adk_protobuf::channels::channel_update_status::ChannelStatus as ChannelUpdateStatusKind;
use adk_protobuf::channels::{
    ChannelStatus, ChannelType, ChannelUpdateGreeting, ChannelUpdateSafetyFilters,
    ChannelUpdateStatus, ChannelUpdateStylePrompt, StylePromptUpdateStylePrompt,
    VoiceChannelUpdateDisclaimer, WebChatChannelUpdateStatus,
};
use adk_protobuf::command::Payload as CommandPayload;
use adk_types::ResourceMap;
use serde_yaml_ng::Value as YamlValue;

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
    ) && let Some(yaml) = resource_yaml(resources, VOICE_SAFETY_FILTERS_FILE.file_path)
    {
        push_channel_safety_filters_update(commands, metadata, ChannelType::Voice, &yaml);
    }

    if resource_changed(
        resources,
        remote_resources,
        VOICE_CONFIGURATION_FILE.file_path,
    ) && let Some(yaml) = resource_yaml(resources, VOICE_CONFIGURATION_FILE.file_path)
    {
        if let Some(greeting) = yaml.get("greeting") {
            push_command(
                commands,
                metadata,
                "channel_update_greeting",
                CommandPayload::ChannelUpdateGreeting(ChannelUpdateGreeting {
                    channel_type: ChannelType::Voice as i32,
                    greeting: Some(GreetingUpdateGreeting {
                        welcome_message: Some(yaml_str(greeting, "welcome_message")),
                        references: None,
                        language_code: yaml_str(greeting, "language_code"),
                    }),
                }),
            );
        }
        if let Some(style_prompt) = yaml.get("style_prompt") {
            push_command(
                commands,
                metadata,
                "channel_update_style_prompt",
                CommandPayload::ChannelUpdateStylePrompt(ChannelUpdateStylePrompt {
                    channel_type: ChannelType::Voice as i32,
                    style_prompt: Some(StylePromptUpdateStylePrompt {
                        prompt: yaml_str(style_prompt, "prompt"),
                    }),
                }),
            );
        }
        if let Some(disclaimer) = yaml.get("disclaimer_messages") {
            let disclaimer = first_yaml_mapping(disclaimer).unwrap_or(disclaimer);
            push_command(
                commands,
                metadata,
                "voice_channel_update_disclaimer",
                CommandPayload::VoiceChannelUpdateDisclaimer(VoiceChannelUpdateDisclaimer {
                    disclaimer: Some(DisclaimerMessageUpdateDisclaimerMessage {
                        message: Some(yaml_str(disclaimer, "message")),
                        is_enabled: Some(
                            disclaimer
                                .get("enabled")
                                .or_else(|| disclaimer.get("is_enabled"))
                                .and_then(YamlValue::as_bool)
                                .unwrap_or(false),
                        ),
                        ringing_tone: None,
                        language_code: yaml_str(disclaimer, "language_code"),
                        references: None,
                    }),
                }),
            );
        }
    }

    let chat_configuration_yaml = if resource_changed(
        resources,
        remote_resources,
        CHAT_CONFIGURATION_FILE.file_path,
    ) {
        resource_yaml(resources, CHAT_CONFIGURATION_FILE.file_path)
    } else {
        None
    };

    if webchat_config_resource_created(resources, remote_resources) {
        push_webchat_channel_status_update(commands, metadata, true);
    }

    if let Some(yaml) = chat_configuration_yaml.as_ref()
        && let Some(greeting) = yaml.get("greeting")
    {
        push_command(
            commands,
            metadata,
            "channel_update_greeting",
            CommandPayload::ChannelUpdateGreeting(ChannelUpdateGreeting {
                channel_type: ChannelType::WebChat as i32,
                greeting: Some(GreetingUpdateGreeting {
                    welcome_message: Some(yaml_str(greeting, "welcome_message")),
                    references: None,
                    language_code: yaml_str(greeting, "language_code"),
                }),
            }),
        );
    }

    if resource_changed(
        resources,
        remote_resources,
        CHAT_SAFETY_FILTERS_FILE.file_path,
    ) && let Some(yaml) = resource_yaml(resources, CHAT_SAFETY_FILTERS_FILE.file_path)
    {
        push_channel_safety_filters_update(commands, metadata, ChannelType::WebChat, &yaml);
    }

    if let Some(yaml) = chat_configuration_yaml.as_ref()
        && let Some(style_prompt) = yaml.get("style_prompt")
    {
        push_command(
            commands,
            metadata,
            "channel_update_style_prompt",
            CommandPayload::ChannelUpdateStylePrompt(ChannelUpdateStylePrompt {
                channel_type: ChannelType::WebChat as i32,
                style_prompt: Some(StylePromptUpdateStylePrompt {
                    prompt: yaml_str(style_prompt, "prompt"),
                }),
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
    yaml: &YamlValue,
) {
    let parsed = match channel_type {
        ChannelType::Voice => {
            VoiceSafetyFilters::parse_local_yaml(VOICE_SAFETY_FILTERS_FILE.file_path, yaml)
        }
        ChannelType::WebChat => {
            ChatSafetyFilters::parse_local_yaml(CHAT_SAFETY_FILTERS_FILE.file_path, yaml)
        }
    };
    let Ok(safety_filters) = parsed else {
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
