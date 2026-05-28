use crate::command_gen::local_file_helpers::{first_yaml_mapping, resource_changed, resource_yaml};
use crate::specs::{
    CHAT_CONFIGURATION_FILE, CHAT_SAFETY_FILTERS_FILE, VOICE_CONFIGURATION_FILE,
    VOICE_SAFETY_FILTERS_FILE,
};
use crate::{push_command, yaml_str};
use adk_protobuf::Metadata;
use adk_protobuf::agent::{DisclaimerMessageUpdateDisclaimerMessage, GreetingUpdateGreeting};
use adk_protobuf::channels::{
    ChannelType, ChannelUpdateGreeting, ChannelUpdateSafetyFilters, ChannelUpdateStylePrompt,
    StylePromptUpdateStylePrompt, VoiceChannelUpdateDisclaimer,
};
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::content_filter_settings::{
    AzureContentFilter, AzureContentFilterCategory,
    ContentFilterSettingsUpdateContentFilterSettings,
};
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
