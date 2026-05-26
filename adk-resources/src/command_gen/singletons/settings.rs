use super::super::local_file_helpers::{first_yaml_mapping, resource_changed, resource_yaml};
use crate::specs::{
    AGENT_PERSONALITY_FILE, AGENT_ROLE_FILE, AGENT_SAFETY_FILTERS_FILE, ASR_SETTINGS_FILE,
    CHAT_CONFIGURATION_FILE, CHAT_SAFETY_FILTERS_FILE, VOICE_CONFIGURATION_FILE,
    VOICE_SAFETY_FILTERS_FILE,
};
use crate::{push_command, yaml_str};
use adk_protobuf::Metadata;
use adk_protobuf::agent::{
    Adjectives, DisclaimerMessageUpdateDisclaimerMessage, GreetingUpdateGreeting,
    PersonalityUpdatePersonality, RoleUpdateRole,
};
use adk_protobuf::asr_settings::{AsrSettingsUpdateAsrSettings, LatencyConfig};
use adk_protobuf::channels::{
    ChannelType, ChannelUpdateGreeting, ChannelUpdateSafetyFilters, ChannelUpdateStylePrompt,
    StylePromptUpdateStylePrompt, VoiceChannelUpdateAsrSettings, VoiceChannelUpdateDisclaimer,
};
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::content_filter_settings::{
    AzureContentFilter, AzureContentFilterCategory,
    ContentFilterSettingsUpdateContentFilterSettings,
};
use adk_types::ResourceMap;
use std::collections::HashMap;

pub(super) fn append_agent_settings_updates(
    commands: &mut Vec<adk_protobuf::Command>,
    resources: &ResourceMap,
    remote_resources: &ResourceMap,
    metadata: &Option<Metadata>,
) {
    if resource_changed(
        resources,
        remote_resources,
        AGENT_PERSONALITY_FILE.file_path,
    ) && let Some(yaml) = resource_yaml(resources, AGENT_PERSONALITY_FILE.file_path)
    {
        let values = yaml
            .get("adjectives")
            .and_then(serde_yaml::Value::as_mapping)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|(key, value)| Some((key.as_str()?.to_string(), value.as_bool()?)))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        push_command(
            commands,
            metadata,
            "update_personality",
            CommandPayload::UpdatePersonality(PersonalityUpdatePersonality {
                adjectives: Some(Adjectives { values }),
                custom: Some(yaml_str(&yaml, "custom")),
                references: None,
            }),
        );
    }

    if resource_changed(resources, remote_resources, AGENT_ROLE_FILE.file_path)
        && let Some(yaml) = resource_yaml(resources, AGENT_ROLE_FILE.file_path)
    {
        push_command(
            commands,
            metadata,
            "update_role",
            CommandPayload::UpdateRole(RoleUpdateRole {
                value: Some(yaml_str(&yaml, "value")),
                additional_info: Some(yaml_str(&yaml, "additional_info")),
                custom: Some(yaml_str(&yaml, "custom")),
                references: None,
            }),
        );
    }

    if resource_changed(
        resources,
        remote_resources,
        AGENT_SAFETY_FILTERS_FILE.file_path,
    ) && let Some(yaml) = resource_yaml(resources, AGENT_SAFETY_FILTERS_FILE.file_path)
    {
        push_command(
            commands,
            metadata,
            "update_content_filter_settings",
            CommandPayload::UpdateContentFilterSettings(content_filter_settings_from_yaml(&yaml)),
        );
    }
}

pub(super) fn append_channel_settings_updates(
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
                                .and_then(serde_yaml::Value::as_bool)
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

    if resource_changed(resources, remote_resources, ASR_SETTINGS_FILE.file_path)
        && let Some(yaml) = resource_yaml(resources, ASR_SETTINGS_FILE.file_path)
    {
        push_command(
            commands,
            metadata,
            "voice_channel_update_asr_settings",
            CommandPayload::VoiceChannelUpdateAsrSettings(VoiceChannelUpdateAsrSettings {
                asr_settings: Some(AsrSettingsUpdateAsrSettings {
                    barge_in: Some(
                        yaml.get("barge_in")
                            .and_then(serde_yaml::Value::as_bool)
                            .unwrap_or(false),
                    ),
                    latency_config: Some(LatencyConfig {
                        interaction_style: yaml_str(&yaml, "interaction_style"),
                    }),
                }),
            }),
        );
    }
}

fn push_channel_safety_filters_update(
    commands: &mut Vec<adk_protobuf::Command>,
    metadata: &Option<Metadata>,
    channel_type: ChannelType,
    yaml: &serde_yaml::Value,
) {
    push_command(
        commands,
        metadata,
        "channel_update_safety_filters",
        CommandPayload::ChannelUpdateSafetyFilters(ChannelUpdateSafetyFilters {
            channel_type: channel_type as i32,
            safety_filters: Some(content_filter_settings_from_yaml(yaml)),
        }),
    );
}

fn content_filter_settings_from_yaml(
    yaml: &serde_yaml::Value,
) -> ContentFilterSettingsUpdateContentFilterSettings {
    let categories = yaml
        .get("categories")
        .and_then(serde_yaml::Value::as_mapping);
    ContentFilterSettingsUpdateContentFilterSettings {
        r#type: Some("azure".to_string()),
        disabled: Some(
            !yaml
                .get("enabled")
                .and_then(serde_yaml::Value::as_bool)
                .unwrap_or(true),
        ),
        azure_config: Some(AzureContentFilter {
            violence: content_filter_category_from_yaml(categories, "violence"),
            hate: content_filter_category_from_yaml(categories, "hate"),
            sexual: content_filter_category_from_yaml(categories, "sexual"),
            self_harm: content_filter_category_from_yaml(categories, "self_harm"),
        }),
    }
}

fn content_filter_category_from_yaml(
    categories: Option<&serde_yaml::Mapping>,
    name: &str,
) -> Option<AzureContentFilterCategory> {
    let category = categories?.get(serde_yaml::Value::String(name.to_string()))?;
    Some(AzureContentFilterCategory {
        is_active: category
            .get("enabled")
            .and_then(serde_yaml::Value::as_bool)
            .unwrap_or(false),
        precision: yaml_str(category, "level").to_ascii_uppercase(),
    })
}
