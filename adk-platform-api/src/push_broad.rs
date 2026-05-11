//! Push commands for broad, single-file resource families that are not part of the original
//! topic/function/entity phase-1 set.

use crate::push_extended::CommandGroups;
use crate::{
    generated_replay_resource_id, projection_to_resource_map, push_command, random_resource_id,
    yaml_str,
};
use adk_domain::ResourceMap;
use adk_protobuf::Metadata;
use adk_protobuf::agent::{
    Adjectives, DisclaimerMessageUpdateDisclaimerMessage, GreetingUpdateGreeting,
    PersonalityUpdatePersonality, RoleUpdateRole,
};
use adk_protobuf::api_integrations::{
    ApiIntegrationConfig, ApiIntegrationCreate, ApiIntegrationOperationCreate, Environments,
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
use adk_protobuf::keyphrase_boosting::KeyphraseBoostingCreateKeyphrase;
use adk_protobuf::pronunciations::PronunciationsCreatePronunciation;
use adk_protobuf::transcript_corrections::{
    RegularExpression, TranscriptCorrectionsCreateTranscriptCorrections,
};
use adk_protobuf::variant::{
    AttributeReferences, AttributeValues, VariantCreateAttribute, VariantCreateVariant,
    VariantValues,
};
use serde_json::{Value, json};
use std::collections::HashMap;
use uuid::Uuid;

pub(crate) fn broad_resource_command_groups(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> CommandGroups {
    let remote_resources = projection_to_resource_map(projection).unwrap_or_default();
    let mut groups = CommandGroups::default();
    let mut api_operation_commands = Vec::new();

    if resource_changed(
        resources,
        &remote_resources,
        "config/variant_attributes.yaml",
    ) && let Some(yaml) = resource_yaml(resources, "config/variant_attributes.yaml")
    {
        let mut variant_ids = HashMap::new();
        if let Some(variants) = yaml
            .get("variants")
            .and_then(serde_yaml::Value::as_sequence)
        {
            for variant in variants {
                let name = yaml_str(variant, "name");
                if name.is_empty() {
                    continue;
                }
                let id = generated_replay_resource_id(
                    "variant",
                    &name,
                    &format!("config/variant_attributes.yaml/variants/{name}"),
                )
                .unwrap_or_else(|| random_resource_id("VARIANTS"));
                variant_ids.insert(name.clone(), id.clone());
                push_command(
                    &mut groups.creates,
                    metadata,
                    "variant_create_variant",
                    CommandPayload::VariantCreateVariant(VariantCreateVariant {
                        id,
                        name,
                        attribute_values: Some(AttributeValues {
                            values: HashMap::new(),
                        }),
                    }),
                );
            }
        }
        if let Some(attributes) = yaml
            .get("attributes")
            .and_then(serde_yaml::Value::as_sequence)
        {
            for attribute in attributes {
                let name = yaml_str(attribute, "name");
                if name.is_empty() {
                    continue;
                }
                let id = generated_replay_resource_id(
                    "variant_attribute",
                    &name,
                    &format!("config/variant_attributes.yaml/attributes/{name}"),
                )
                .unwrap_or_else(|| random_resource_id("VARIANT_ATTRIBUTES"));
                let mut values = HashMap::new();
                if let Some(attribute_values) = attribute
                    .get("values")
                    .and_then(serde_yaml::Value::as_mapping)
                {
                    for (variant_name, value) in attribute_values {
                        let Some(variant_name) = variant_name.as_str() else {
                            continue;
                        };
                        let Some(variant_id) = variant_ids.get(variant_name) else {
                            continue;
                        };
                        values.insert(
                            variant_id.clone(),
                            value.as_str().unwrap_or_default().to_string(),
                        );
                    }
                }
                push_command(
                    &mut groups.creates,
                    metadata,
                    "variant_create_attribute",
                    CommandPayload::VariantCreateAttribute(VariantCreateAttribute {
                        id,
                        name,
                        references: Some(AttributeReferences {
                            topics: HashMap::new(),
                            flow_steps: HashMap::new(),
                            no_code_steps: HashMap::new(),
                        }),
                        variant_values: Some(VariantValues { values }),
                    }),
                );
            }
        }
    }

    if resource_changed(resources, &remote_resources, "config/api_integrations.yaml")
        && let Some(yaml) = resource_yaml(resources, "config/api_integrations.yaml")
        && let Some(integrations) = yaml
            .get("api_integrations")
            .and_then(serde_yaml::Value::as_sequence)
    {
        for integration in integrations {
            let name = yaml_str(integration, "name");
            if name.is_empty() {
                continue;
            }
            let id = generated_replay_resource_id(
                "api_integration",
                &name,
                &format!("config/api_integrations.yaml/api_integrations/{name}"),
            )
            .unwrap_or_else(|| random_resource_id("API-INTEGRATION"));
            push_command(
                &mut groups.creates,
                metadata,
                "create_api_integration",
                CommandPayload::CreateApiIntegration(ApiIntegrationCreate {
                    id: id.clone(),
                    name: name.clone(),
                    description: Some(yaml_str(integration, "description")),
                    environments: api_integration_environments(integration),
                }),
            );
            if let Some(operations) = integration
                .get("operations")
                .and_then(serde_yaml::Value::as_sequence)
            {
                for operation in operations {
                    let op_name = yaml_str(operation, "name");
                    if op_name.is_empty() {
                        continue;
                    }
                    let op_id = generated_replay_resource_id(
                        "api_integration_operation",
                        &op_name,
                        &format!(
                            "config/api_integrations.yaml/api_integrations/{name}/operations/{op_name}"
                        ),
                    )
                    .unwrap_or_else(|| Uuid::new_v4().to_string());
                    push_command(
                        &mut api_operation_commands,
                        metadata,
                        "create_api_integration_operation",
                        CommandPayload::CreateApiIntegrationOperation(
                            ApiIntegrationOperationCreate {
                                id: op_id,
                                integration_id: id.clone(),
                                name: op_name,
                                method: yaml_str(operation, "method"),
                                resource: yaml_str(operation, "resource"),
                            },
                        ),
                    );
                }
            }
        }
    }

    if resource_changed(
        resources,
        &remote_resources,
        "voice/speech_recognition/keyphrase_boosting.yaml",
    ) && let Some(yaml) = resource_yaml(
        resources,
        "voice/speech_recognition/keyphrase_boosting.yaml",
    ) && let Some(keyphrases) = yaml
        .get("keyphrases")
        .and_then(serde_yaml::Value::as_sequence)
    {
        for keyphrase in keyphrases {
            let text = yaml_str(keyphrase, "keyphrase");
            if text.is_empty() {
                continue;
            }
            let id = generated_replay_resource_id(
                "keyphrase_boosting",
                &text,
                &format!("voice/speech_recognition/keyphrase_boosting.yaml/keyphrases/{text}"),
            )
            .unwrap_or_else(|| random_resource_id("KEYPHRASE_BOOSTING"));
            push_command(
                &mut groups.creates,
                metadata,
                "create_keyphrase_boosting",
                CommandPayload::CreateKeyphraseBoosting(KeyphraseBoostingCreateKeyphrase {
                    id,
                    keyphrase: text,
                    level: yaml_str(keyphrase, "level"),
                }),
            );
        }
    }

    if resource_changed(
        resources,
        &remote_resources,
        "voice/speech_recognition/transcript_corrections.yaml",
    ) && let Some(yaml) = resource_yaml(
        resources,
        "voice/speech_recognition/transcript_corrections.yaml",
    ) && let Some(corrections) = yaml
        .get("corrections")
        .and_then(serde_yaml::Value::as_sequence)
    {
        for correction in corrections {
            let name = yaml_str(correction, "name");
            if name.is_empty() {
                continue;
            }
            let id = generated_replay_resource_id(
                "transcript_corrections",
                &name,
                &format!("voice/speech_recognition/transcript_corrections.yaml/corrections/{name}"),
            )
            .unwrap_or_else(|| random_resource_id("TRANSCRIPT_CORRECTIONS"));
            let regular_expressions = correction
                .get("regular_expressions")
                .and_then(serde_yaml::Value::as_sequence)
                .map(|items| {
                    items
                        .iter()
                        .enumerate()
                        .map(|(idx, regex)| RegularExpression {
                            id: format!("{id}-REGEX-{idx}"),
                            regular_expression: yaml_str(regex, "regular_expression"),
                            replacement: yaml_str(regex, "replacement"),
                            replacement_type: yaml_str(regex, "replacement_type"),
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            push_command(
                &mut groups.creates,
                metadata,
                "create_transcript_corrections",
                CommandPayload::CreateTranscriptCorrections(
                    TranscriptCorrectionsCreateTranscriptCorrections {
                        id,
                        name,
                        description: Some(yaml_str(correction, "description")),
                        regular_expressions,
                    },
                ),
            );
        }
    }

    if resource_changed(
        resources,
        &remote_resources,
        "voice/response_control/pronunciations.yaml",
    ) && let Some(yaml) = resource_yaml(resources, "voice/response_control/pronunciations.yaml")
        && let Some(pronunciations) = yaml
            .get("pronunciations")
            .and_then(serde_yaml::Value::as_sequence)
    {
        for (position, pronunciation) in pronunciations.iter().enumerate() {
            let regex = yaml_str(pronunciation, "regex");
            if regex.is_empty() {
                continue;
            }
            let id = generated_replay_resource_id(
                "pronunciations",
                &regex,
                &format!("voice/response_control/pronunciations.yaml/pronunciations/{regex}"),
            )
            .unwrap_or_else(|| random_resource_id("PRONUNCIATIONS"));
            push_command(
                &mut groups.creates,
                metadata,
                "pronunciations_create_pronunciation",
                CommandPayload::PronunciationsCreatePronunciation(
                    PronunciationsCreatePronunciation {
                        id,
                        regex,
                        replacement: yaml_str(pronunciation, "replacement"),
                        case_sensitive: pronunciation
                            .get("case_sensitive")
                            .and_then(serde_yaml::Value::as_bool)
                            .unwrap_or(false),
                        language_code: yaml_str(pronunciation, "language_code"),
                        description: yaml_str(pronunciation, "description"),
                        position: position as i32,
                        name: yaml_str(pronunciation, "name"),
                    },
                ),
            );
        }
    }

    groups.creates.extend(api_operation_commands);

    if resource_changed(
        resources,
        &remote_resources,
        "agent_settings/personality.yaml",
    ) && let Some(yaml) = resource_yaml(resources, "agent_settings/personality.yaml")
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
            &mut groups.updates,
            metadata,
            "update_personality",
            CommandPayload::UpdatePersonality(PersonalityUpdatePersonality {
                adjectives: Some(Adjectives { values }),
                custom: Some(yaml_str(&yaml, "custom")),
                references: None,
            }),
        );
    }

    if resource_changed(resources, &remote_resources, "agent_settings/role.yaml")
        && let Some(yaml) = resource_yaml(resources, "agent_settings/role.yaml")
    {
        push_command(
            &mut groups.updates,
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
        &remote_resources,
        "agent_settings/safety_filters.yaml",
    ) && let Some(yaml) = resource_yaml(resources, "agent_settings/safety_filters.yaml")
    {
        push_command(
            &mut groups.updates,
            metadata,
            "update_content_filter_settings",
            CommandPayload::UpdateContentFilterSettings(content_filter_settings_from_yaml(&yaml)),
        );
    }

    if resource_changed(resources, &remote_resources, "voice/safety_filters.yaml")
        && let Some(yaml) = resource_yaml(resources, "voice/safety_filters.yaml")
    {
        push_channel_safety_filters_update(
            &mut groups.updates,
            metadata,
            ChannelType::Voice,
            &yaml,
        );
    }

    if resource_changed(resources, &remote_resources, "voice/configuration.yaml")
        && let Some(yaml) = resource_yaml(resources, "voice/configuration.yaml")
    {
        if let Some(greeting) = yaml.get("greeting") {
            push_command(
                &mut groups.updates,
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
                &mut groups.updates,
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
                &mut groups.updates,
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

    let chat_configuration_yaml =
        if resource_changed(resources, &remote_resources, "chat/configuration.yaml") {
            resource_yaml(resources, "chat/configuration.yaml")
        } else {
            None
        };

    if let Some(yaml) = chat_configuration_yaml.as_ref() {
        if let Some(greeting) = yaml.get("greeting") {
            push_command(
                &mut groups.updates,
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
    }

    if resource_changed(resources, &remote_resources, "chat/safety_filters.yaml")
        && let Some(yaml) = resource_yaml(resources, "chat/safety_filters.yaml")
    {
        push_channel_safety_filters_update(
            &mut groups.updates,
            metadata,
            ChannelType::WebChat,
            &yaml,
        );
    }

    if let Some(yaml) = chat_configuration_yaml.as_ref() {
        if let Some(style_prompt) = yaml.get("style_prompt") {
            push_command(
                &mut groups.updates,
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

    if resource_changed(
        resources,
        &remote_resources,
        "voice/speech_recognition/asr_settings.yaml",
    ) && let Some(yaml) = resource_yaml(resources, "voice/speech_recognition/asr_settings.yaml")
    {
        push_command(
            &mut groups.updates,
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

    groups
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

pub(crate) fn payload_json_summary(payload: &CommandPayload) -> Option<(&'static str, Value)> {
    match payload {
        CommandPayload::VariantCreateVariant(msg) => Some((
            "variant_create_variant",
            json!({
                "id": msg.id,
                "name": msg.name,
                "attribute_values": attribute_values_json(msg.attribute_values.as_ref()),
            }),
        )),
        CommandPayload::VariantCreateAttribute(msg) => Some((
            "variant_create_attribute",
            json!({
                "id": msg.id,
                "name": msg.name,
                "references": attribute_references_json(msg.references.as_ref()),
                "variant_values": {
                    "values": msg
                        .variant_values
                        .as_ref()
                        .map(|values| json!(values.values))
                        .unwrap_or_else(|| json!({})),
                },
            }),
        )),
        CommandPayload::CreateApiIntegration(msg) => Some((
            "create_api_integration",
            json!({
                "id": msg.id,
                "name": msg.name,
                "description": msg.description.clone().unwrap_or_default(),
                "environments": environments_json(msg.environments.as_ref()),
            }),
        )),
        CommandPayload::CreateApiIntegrationOperation(msg) => Some((
            "create_api_integration_operation",
            json!({
                "id": msg.id,
                "integration_id": msg.integration_id,
                "name": msg.name,
                "method": msg.method,
                "resource": msg.resource,
            }),
        )),
        CommandPayload::CreateKeyphraseBoosting(msg) => Some((
            "create_keyphrase_boosting",
            json!({
                "id": msg.id,
                "keyphrase": msg.keyphrase,
                "level": msg.level,
            }),
        )),
        CommandPayload::CreateTranscriptCorrections(msg) => Some((
            "create_transcript_corrections",
            json!({
                "id": msg.id,
                "name": msg.name,
                "description": msg.description.clone().unwrap_or_default(),
                "regular_expressions": msg.regular_expressions.iter().map(regular_expression_json).collect::<Vec<_>>(),
            }),
        )),
        CommandPayload::PronunciationsCreatePronunciation(msg) => Some((
            "pronunciations_create_pronunciation",
            json!({
                "id": msg.id,
                "regex": msg.regex,
                "replacement": msg.replacement,
                "case_sensitive": msg.case_sensitive,
                "language_code": msg.language_code,
            }),
        )),
        CommandPayload::UpdatePersonality(msg) => Some((
            "update_personality",
            json!({
                "adjectives": {
                    "values": msg
                        .adjectives
                        .as_ref()
                        .map(|adjectives| json!(adjectives.values))
                        .unwrap_or_else(|| json!({})),
                },
                "custom": msg.custom.clone().unwrap_or_default(),
            }),
        )),
        CommandPayload::UpdateRole(msg) => Some((
            "update_role",
            json!({
                "value": msg.value.clone().unwrap_or_default(),
                "additional_info": msg.additional_info.clone().unwrap_or_default(),
                "custom": msg.custom.clone().unwrap_or_default(),
            }),
        )),
        CommandPayload::UpdateContentFilterSettings(msg) => Some((
            "update_content_filter_settings",
            content_filter_settings_json(msg),
        )),
        CommandPayload::ChannelUpdateGreeting(msg) => Some((
            "channel_update_greeting",
            channel_payload_json(
                msg.channel_type,
                "greeting",
                msg.greeting
                    .as_ref()
                    .map(greeting_json)
                    .unwrap_or_else(|| json!({})),
            ),
        )),
        CommandPayload::ChannelUpdateStylePrompt(msg) => Some((
            "channel_update_style_prompt",
            channel_payload_json(
                msg.channel_type,
                "style_prompt",
                msg.style_prompt
                    .as_ref()
                    .map(|style_prompt| json!({ "prompt": style_prompt.prompt }))
                    .unwrap_or_else(|| json!({})),
            ),
        )),
        CommandPayload::ChannelUpdateSafetyFilters(msg) => Some((
            "channel_update_safety_filters",
            channel_payload_json(
                msg.channel_type,
                "safety_filters",
                msg.safety_filters
                    .as_ref()
                    .map(content_filter_settings_json)
                    .unwrap_or_else(|| json!({})),
            ),
        )),
        CommandPayload::VoiceChannelUpdateDisclaimer(msg) => Some((
            "voice_channel_update_disclaimer",
            json!({
                "disclaimer": msg
                    .disclaimer
                    .as_ref()
                    .map(disclaimer_json)
                    .unwrap_or_else(|| json!({})),
            }),
        )),
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

fn resource_yaml(resources: &ResourceMap, path: &str) -> Option<serde_yaml::Value> {
    let content = resources.get(path)?.payload.get("content")?.as_str()?;
    serde_yaml::from_str(content).ok()
}

fn first_yaml_mapping(value: &serde_yaml::Value) -> Option<&serde_yaml::Value> {
    value
        .as_sequence()?
        .iter()
        .find(|item| item.as_mapping().is_some())
}

fn resource_changed(local: &ResourceMap, remote: &ResourceMap, path: &str) -> bool {
    let Some(local_content) = local
        .get(path)
        .and_then(|resource| resource.payload.get("content"))
        .and_then(Value::as_str)
    else {
        return false;
    };
    let Some(remote_content) = remote
        .get(path)
        .and_then(|resource| resource.payload.get("content"))
        .and_then(Value::as_str)
    else {
        return true;
    };
    local_content != remote_content
}

fn api_integration_environments(integration: &serde_yaml::Value) -> Option<Environments> {
    let envs = integration
        .get("environments")
        .and_then(serde_yaml::Value::as_mapping)?;
    let sandbox = api_integration_environment(envs, &["sandbox"]);
    let pre_release = api_integration_environment(envs, &["pre-release", "pre_release"]);
    let live = api_integration_environment(envs, &["live"]);
    if sandbox.is_none() && pre_release.is_none() && live.is_none() {
        return None;
    }
    Some(Environments {
        sandbox,
        pre_release,
        live,
    })
}

fn api_integration_environment(
    envs: &serde_yaml::Mapping,
    keys: &[&str],
) -> Option<ApiIntegrationConfig> {
    let env = keys
        .iter()
        .find_map(|key| envs.get(serde_yaml::Value::String((*key).to_string())))?;
    Some(ApiIntegrationConfig {
        base_url: yaml_str(env, "base_url"),
        auth_type: yaml_str(env, "auth_type"),
    })
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

fn attribute_values_json(values: Option<&AttributeValues>) -> Value {
    let Some(values) = values else {
        return json!({});
    };
    if values.values.is_empty() {
        json!({})
    } else {
        json!({ "values": values.values })
    }
}

fn attribute_references_json(references: Option<&AttributeReferences>) -> Value {
    let Some(references) = references else {
        return json!({});
    };
    let mut value = serde_json::Map::new();
    if !references.topics.is_empty() {
        value.insert("topics".to_string(), json!(references.topics));
    }
    if !references.flow_steps.is_empty() {
        value.insert("flow_steps".to_string(), json!(references.flow_steps));
    }
    if !references.no_code_steps.is_empty() {
        value.insert("no_code_steps".to_string(), json!(references.no_code_steps));
    }
    Value::Object(value)
}

fn environments_json(environments: Option<&Environments>) -> Value {
    let Some(environments) = environments else {
        return json!({});
    };
    let mut value = serde_json::Map::new();
    if let Some(sandbox) = &environments.sandbox {
        value.insert("sandbox".to_string(), api_integration_config_json(sandbox));
    }
    if let Some(pre_release) = &environments.pre_release {
        value.insert(
            "pre_release".to_string(),
            api_integration_config_json(pre_release),
        );
    }
    if let Some(live) = &environments.live {
        value.insert("live".to_string(), api_integration_config_json(live));
    }
    Value::Object(value)
}

fn api_integration_config_json(config: &ApiIntegrationConfig) -> Value {
    json!({
        "base_url": config.base_url,
        "auth_type": config.auth_type,
    })
}

fn regular_expression_json(regex: &RegularExpression) -> Value {
    json!({
        "id": regex.id,
        "regular_expression": regex.regular_expression,
        "replacement": regex.replacement,
        "replacement_type": regex.replacement_type,
    })
}

fn content_filter_settings_json(
    settings: &ContentFilterSettingsUpdateContentFilterSettings,
) -> Value {
    let mut value = serde_json::Map::new();
    value.insert(
        "type".to_string(),
        Value::String(settings.r#type.clone().unwrap_or_default()),
    );
    value.insert(
        "disabled".to_string(),
        Value::Bool(settings.disabled.unwrap_or(false)),
    );
    if let Some(azure_config) = &settings.azure_config {
        value.insert(
            "azure_config".to_string(),
            azure_content_filter_json(azure_config),
        );
    }
    Value::Object(value)
}

fn azure_content_filter_json(filter: &AzureContentFilter) -> Value {
    let mut value = serde_json::Map::new();
    if let Some(hate) = &filter.hate {
        value.insert("hate".to_string(), content_filter_category_json(hate));
    }
    if let Some(self_harm) = &filter.self_harm {
        value.insert(
            "self_harm".to_string(),
            content_filter_category_json(self_harm),
        );
    }
    if let Some(sexual) = &filter.sexual {
        value.insert("sexual".to_string(), content_filter_category_json(sexual));
    }
    if let Some(violence) = &filter.violence {
        value.insert(
            "violence".to_string(),
            content_filter_category_json(violence),
        );
    }
    Value::Object(value)
}

fn content_filter_category_json(category: &AzureContentFilterCategory) -> Value {
    let mut value = serde_json::Map::new();
    if category.is_active {
        value.insert("is_active".to_string(), Value::Bool(true));
    }
    value.insert(
        "precision".to_string(),
        Value::String(category.precision.clone()),
    );
    Value::Object(value)
}

fn channel_payload_json(channel_type: i32, payload_key: &str, payload: Value) -> Value {
    let mut value = serde_json::Map::new();
    if channel_type == ChannelType::WebChat as i32 {
        value.insert(
            "channel_type".to_string(),
            Value::String("WEB_CHAT".to_string()),
        );
    }
    value.insert(payload_key.to_string(), payload);
    Value::Object(value)
}

fn greeting_json(greeting: &GreetingUpdateGreeting) -> Value {
    json!({
        "welcome_message": greeting.welcome_message.clone().unwrap_or_default(),
        "language_code": greeting.language_code,
    })
}

fn disclaimer_json(disclaimer: &DisclaimerMessageUpdateDisclaimerMessage) -> Value {
    json!({
        "message": disclaimer.message.clone().unwrap_or_default(),
        "is_enabled": disclaimer.is_enabled.unwrap_or(false),
        "language_code": disclaimer.language_code,
    })
}

fn asr_settings_json(settings: &AsrSettingsUpdateAsrSettings) -> Value {
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
