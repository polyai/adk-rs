use crate::channels::local::parse_channel_configuration;
use crate::channels::{ChatSafetyFilters, VoiceSafetyFilters};
use crate::local_parse::ParseLocalResource;
use crate::push_command;
use crate::push_command_inputs::{resource_changed, resource_yaml};
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
        && let Ok(config) = parse_channel_configuration(VOICE_CONFIGURATION_FILE.file_path, &yaml)
    {
        if let Some(greeting) = config.greeting() {
            push_command(
                commands,
                metadata,
                "channel_update_greeting",
                CommandPayload::ChannelUpdateGreeting(ChannelUpdateGreeting {
                    channel_type: ChannelType::Voice as i32,
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
                    channel_type: ChannelType::Voice as i32,
                    style_prompt: Some(style_prompt.to_update_proto()),
                }),
            );
        }
        if let Some(disclaimer) = config.disclaimer() {
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

    let chat_configuration = if resource_changed(
        resources,
        remote_resources,
        CHAT_CONFIGURATION_FILE.file_path,
    ) {
        resource_yaml(resources, CHAT_CONFIGURATION_FILE.file_path).and_then(|yaml| {
            parse_channel_configuration(CHAT_CONFIGURATION_FILE.file_path, &yaml).ok()
        })
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
    ) && let Some(yaml) = resource_yaml(resources, CHAT_SAFETY_FILTERS_FILE.file_path)
    {
        push_channel_safety_filters_update(commands, metadata, ChannelType::WebChat, &yaml);
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
