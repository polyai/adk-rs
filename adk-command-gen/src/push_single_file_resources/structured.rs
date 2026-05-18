//! Push commands for structured single-file resource families such as variants, API integrations,
//! pronunciation data, transcript corrections, keyphrase boosting, and channel settings.

use super::CommandGroups;
use crate::{
    generated_replay_resource_id, projection_to_resource_map, push_command, random_resource_id,
    yaml_str,
};
use adk_protobuf::Metadata;
use adk_protobuf::agent::{
    Adjectives, DisclaimerMessageUpdateDisclaimerMessage, GreetingUpdateGreeting,
    PersonalityUpdatePersonality, RoleUpdateRole,
};
use adk_protobuf::api_integrations::{
    ApiIntegrationConfig, ApiIntegrationConfigUpdate, ApiIntegrationCreate, ApiIntegrationDelete,
    ApiIntegrationOperationCreate, ApiIntegrationOperationDelete, ApiIntegrationOperationUpdate,
    ApiIntegrationUpdate, Environments,
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
use adk_protobuf::keyphrase_boosting::{
    KeyphraseBoostingCreateKeyphrase, KeyphraseBoostingDeleteKeyphrase,
    KeyphraseBoostingUpdateKeyphrase,
};
use adk_protobuf::pronunciations::{
    PronunciationsCreatePronunciation, PronunciationsDeletePronunciation,
    PronunciationsUpdatePronunciation,
};
use adk_protobuf::transcript_corrections::{
    RegularExpression, TranscriptCorrection, TranscriptCorrectionsCreateTranscriptCorrections,
    TranscriptCorrectionsDeleteTranscriptCorrections, TranscriptCorrectionsUpdateData,
    TranscriptCorrectionsUpdateTranscriptCorrections,
};
use adk_protobuf::variant::{
    AttributeReferences, AttributeValues, VariantCreateAttribute, VariantCreateVariant,
    VariantDeleteAttribute, VariantDeleteVariant, VariantSetDefaultVariant, VariantUpdateAttribute,
    VariantValues,
};
use adk_types::ResourceMap;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

pub(crate) fn structured_file_resource_command_groups(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> CommandGroups {
    let remote_resources = projection_to_resource_map(projection).unwrap_or_default();
    let mut groups = CommandGroups::default();
    let variant_lifecycle = variant_lifecycle_commands(resources, projection, metadata);
    let api_lifecycle = api_integration_lifecycle_commands(resources, projection, metadata);
    let keyphrase_lifecycle = keyphrase_lifecycle_commands(resources, projection, metadata);
    let transcript_lifecycle = transcript_lifecycle_commands(resources, projection, metadata);
    let pronunciation_lifecycle = pronunciation_lifecycle_commands(resources, projection, metadata);

    groups.deletes.extend(variant_lifecycle.variant_deletes);
    groups.deletes.extend(api_lifecycle.integration_deletes);
    groups.deletes.extend(keyphrase_lifecycle.deletes);
    groups.deletes.extend(variant_lifecycle.attribute_deletes);
    groups.deletes.extend(transcript_lifecycle.deletes);
    groups.deletes.extend(pronunciation_lifecycle.deletes);
    groups.deletes.extend(api_lifecycle.operation_deletes);

    groups.creates.extend(variant_lifecycle.variant_creates);
    groups.creates.extend(variant_lifecycle.attribute_creates);
    groups.creates.extend(api_lifecycle.integration_creates);
    groups.creates.extend(keyphrase_lifecycle.creates);
    groups.creates.extend(transcript_lifecycle.creates);
    groups.creates.extend(pronunciation_lifecycle.creates);
    groups.creates.extend(api_lifecycle.operation_creates);

    groups.updates.extend(api_lifecycle.integration_updates);
    groups.updates.extend(variant_lifecycle.variant_updates);
    groups.updates.extend(variant_lifecycle.attribute_updates);
    groups.updates.extend(keyphrase_lifecycle.updates);
    groups.updates.extend(transcript_lifecycle.updates);
    groups.updates.extend(pronunciation_lifecycle.updates);
    groups.updates.extend(api_lifecycle.operation_updates);
    groups.updates.extend(api_lifecycle.config_updates);

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

    if let Some(yaml) = chat_configuration_yaml.as_ref()
        && let Some(greeting) = yaml.get("greeting")
    {
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

    if let Some(yaml) = chat_configuration_yaml.as_ref()
        && let Some(style_prompt) = yaml.get("style_prompt")
    {
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

#[derive(Default)]
struct VariantLifecycleCommands {
    variant_deletes: Vec<adk_protobuf::Command>,
    attribute_deletes: Vec<adk_protobuf::Command>,
    variant_creates: Vec<adk_protobuf::Command>,
    attribute_creates: Vec<adk_protobuf::Command>,
    variant_updates: Vec<adk_protobuf::Command>,
    attribute_updates: Vec<adk_protobuf::Command>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct VariantItem {
    id: String,
    name: String,
    is_default: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct VariantAttributeItem {
    id: String,
    name: String,
    values: HashMap<String, String>,
}

fn variant_lifecycle_commands(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> VariantLifecycleCommands {
    let Some(yaml) = resource_yaml(resources, "config/variant_attributes.yaml") else {
        return VariantLifecycleCommands::default();
    };

    let remote_variants = remote_variant_items(projection);
    let remote_attributes = remote_variant_attribute_items(projection);
    let mut local_variants = local_variant_items(&yaml);
    let mut local_attributes = local_variant_attribute_items(&yaml);

    let remote_variants_by_name = remote_variants
        .iter()
        .map(|item| (item.name.clone(), item.clone()))
        .collect::<HashMap<_, _>>();
    let remote_attributes_by_name = remote_attributes
        .iter()
        .map(|item| (item.name.clone(), item.clone()))
        .collect::<HashMap<_, _>>();
    let local_variant_names = local_variants
        .iter()
        .map(|item| item.name.clone())
        .collect::<HashSet<_>>();
    let local_attribute_names = local_attributes
        .iter()
        .map(|item| item.name.clone())
        .collect::<HashSet<_>>();

    let mut commands = VariantLifecycleCommands::default();
    for remote in &remote_variants {
        if !local_variant_names.contains(&remote.name) {
            push_command(
                &mut commands.variant_deletes,
                metadata,
                "variant_delete_variant",
                CommandPayload::VariantDeleteVariant(VariantDeleteVariant {
                    id: remote.id.clone(),
                }),
            );
        }
    }
    for remote in &remote_attributes {
        if !local_attribute_names.contains(&remote.name) {
            push_command(
                &mut commands.attribute_deletes,
                metadata,
                "variant_delete_attribute",
                CommandPayload::VariantDeleteAttribute(VariantDeleteAttribute {
                    id: remote.id.clone(),
                }),
            );
        }
    }

    let mut variant_ids_by_name = remote_variants_by_name
        .iter()
        .map(|(name, item)| (name.clone(), item.id.clone()))
        .collect::<HashMap<_, _>>();
    let remote_attribute_ids = remote_attributes
        .iter()
        .map(|item| (item.id.clone(), String::new()))
        .collect::<HashMap<_, _>>();

    for local in &mut local_variants {
        if remote_variants_by_name.contains_key(&local.name) {
            continue;
        }
        let id =
            generated_replay_resource_id("variant", &local.name, "config/variant_attributes.yaml")
                .unwrap_or_else(|| random_resource_id("VARIANTS"));
        local.id = id.clone();
        variant_ids_by_name.insert(local.name.clone(), id.clone());
        push_command(
            &mut commands.variant_creates,
            metadata,
            "variant_create_variant",
            CommandPayload::VariantCreateVariant(VariantCreateVariant {
                id,
                name: local.name.clone(),
                attribute_values: Some(AttributeValues {
                    values: remote_attribute_ids.clone(),
                }),
            }),
        );
    }

    let remote_default = remote_variants.iter().find(|item| item.is_default);
    let local_default = local_variants.iter().find(|item| item.is_default);
    if let (Some(remote_default), Some(local_default)) = (remote_default, local_default)
        && remote_default.name != local_default.name
        && let Some(id) = variant_ids_by_name.get(&local_default.name)
    {
        push_command(
            &mut commands.variant_updates,
            metadata,
            "variant_set_default_variant",
            CommandPayload::VariantSetDefaultVariant(VariantSetDefaultVariant { id: id.clone() }),
        );
    }

    for local in &mut local_attributes {
        if remote_attributes_by_name.contains_key(&local.name) {
            continue;
        }
        let id = generated_replay_resource_id(
            "variant_attribute",
            &local.name,
            "config/variant_attributes.yaml",
        )
        .unwrap_or_else(|| random_resource_id("VARIANT_ATTRIBUTES"));
        local.id = id.clone();
        push_command(
            &mut commands.attribute_creates,
            metadata,
            "variant_create_attribute",
            CommandPayload::VariantCreateAttribute(VariantCreateAttribute {
                id,
                name: local.name.clone(),
                references: Some(empty_attribute_references()),
                variant_values: Some(VariantValues {
                    values: variant_attribute_values_with_ids(&local.values, &variant_ids_by_name),
                }),
            }),
        );
    }

    for local in &local_attributes {
        let Some(remote) = remote_attributes_by_name.get(&local.name) else {
            continue;
        };
        let values = variant_attribute_values_with_ids(&local.values, &variant_ids_by_name);
        if values == remote.values {
            continue;
        }
        push_command(
            &mut commands.attribute_updates,
            metadata,
            "variant_update_attribute",
            CommandPayload::VariantUpdateAttribute(VariantUpdateAttribute {
                id: remote.id.clone(),
                name: Some(local.name.clone()),
                references: None,
                variant_values: Some(VariantValues { values }),
            }),
        );
    }

    commands
}

#[derive(Default)]
struct ApiIntegrationLifecycleCommands {
    integration_deletes: Vec<adk_protobuf::Command>,
    operation_deletes: Vec<adk_protobuf::Command>,
    integration_creates: Vec<adk_protobuf::Command>,
    operation_creates: Vec<adk_protobuf::Command>,
    integration_updates: Vec<adk_protobuf::Command>,
    operation_updates: Vec<adk_protobuf::Command>,
    config_updates: Vec<adk_protobuf::Command>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ApiIntegrationItem {
    id: String,
    name: String,
    description: String,
    environments: HashMap<String, ApiEnvironmentItem>,
    operations: Vec<ApiOperationItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ApiEnvironmentItem {
    base_url: String,
    auth_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ApiOperationItem {
    id: String,
    name: String,
    method: String,
    resource: String,
}

fn api_integration_lifecycle_commands(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> ApiIntegrationLifecycleCommands {
    let Some(yaml) = resource_yaml(resources, "config/api_integrations.yaml") else {
        return ApiIntegrationLifecycleCommands::default();
    };

    let local_integrations = local_api_integration_items(&yaml);
    let remote_integrations = remote_api_integration_items(projection);
    let local_names = local_integrations
        .iter()
        .map(|item| item.name.clone())
        .collect::<HashSet<_>>();
    let remote_by_name = remote_integrations
        .iter()
        .map(|item| (item.name.clone(), item.clone()))
        .collect::<HashMap<_, _>>();
    let mut integration_ids_by_name = remote_by_name
        .iter()
        .map(|(name, item)| (name.clone(), item.id.clone()))
        .collect::<HashMap<_, _>>();

    let mut commands = ApiIntegrationLifecycleCommands::default();
    for remote in &remote_integrations {
        if !local_names.contains(&remote.name) {
            push_command(
                &mut commands.integration_deletes,
                metadata,
                "delete_api_integration",
                CommandPayload::DeleteApiIntegration(ApiIntegrationDelete {
                    id: remote.id.clone(),
                }),
            );
        }
    }

    for local in &local_integrations {
        if remote_by_name.contains_key(&local.name) {
            continue;
        }
        let id = generated_replay_resource_id(
            "api_integration",
            &local.name,
            "config/api_integrations.yaml",
        )
        .unwrap_or_else(|| random_resource_id("API-INTEGRATION"));
        integration_ids_by_name.insert(local.name.clone(), id.clone());
        push_command(
            &mut commands.integration_creates,
            metadata,
            "create_api_integration",
            CommandPayload::CreateApiIntegration(ApiIntegrationCreate {
                id,
                name: local.name.clone(),
                description: Some(local.description.clone()),
                environments: Some(environments_from_items(&local.environments)),
            }),
        );
    }

    for local in &local_integrations {
        let Some(remote) = remote_by_name.get(&local.name) else {
            continue;
        };
        if local.description != remote.description {
            push_command(
                &mut commands.integration_updates,
                metadata,
                "update_api_integration",
                CommandPayload::UpdateApiIntegration(ApiIntegrationUpdate {
                    id: remote.id.clone(),
                    name: Some(local.name.clone()),
                    description: Some(local.description.clone()),
                }),
            );
        }

        let local_ops_by_name = local
            .operations
            .iter()
            .map(|operation| (operation.name.clone(), operation.clone()))
            .collect::<HashMap<_, _>>();
        let remote_ops_by_name = remote
            .operations
            .iter()
            .map(|operation| (operation.name.clone(), operation.clone()))
            .collect::<HashMap<_, _>>();
        for remote_operation in &remote.operations {
            if !local_ops_by_name.contains_key(&remote_operation.name) {
                push_command(
                    &mut commands.operation_deletes,
                    metadata,
                    "delete_api_integration_operation",
                    CommandPayload::DeleteApiIntegrationOperation(ApiIntegrationOperationDelete {
                        id: remote_operation.id.clone(),
                        integration_id: remote.id.clone(),
                    }),
                );
            }
        }
        for local_operation in &local.operations {
            match remote_ops_by_name.get(&local_operation.name) {
                Some(remote_operation) => {
                    if local_operation.method != remote_operation.method
                        || local_operation.resource != remote_operation.resource
                    {
                        push_command(
                            &mut commands.operation_updates,
                            metadata,
                            "update_api_integration_operation",
                            CommandPayload::UpdateApiIntegrationOperation(
                                ApiIntegrationOperationUpdate {
                                    id: remote_operation.id.clone(),
                                    name: Some(local_operation.name.clone()),
                                    method: Some(local_operation.method.clone()),
                                    resource: Some(local_operation.resource.clone()),
                                    integration_id: Some(remote.id.clone()),
                                },
                            ),
                        );
                    }
                }
                None => push_create_api_operation(
                    &mut commands.operation_creates,
                    metadata,
                    &remote.id,
                    local_operation,
                ),
            }
        }

        for env_name in ["sandbox", "pre_release", "live"] {
            let local_env = local.environments.get(env_name);
            let remote_env = remote.environments.get(env_name);
            if let Some(local_env) = local_env
                && Some(local_env) != remote_env
            {
                push_command(
                    &mut commands.config_updates,
                    metadata,
                    "update_api_integration_config",
                    CommandPayload::UpdateApiIntegrationConfig(ApiIntegrationConfigUpdate {
                        id: remote.id.clone(),
                        environment: env_name.to_string(),
                        base_url: Some(local_env.base_url.clone()),
                        auth_type: Some(local_env.auth_type.clone()),
                    }),
                );
            }
        }
    }

    for local in &local_integrations {
        if remote_by_name.contains_key(&local.name) {
            continue;
        }
        let integration_id = integration_ids_by_name
            .get(&local.name)
            .cloned()
            .unwrap_or_default();
        for local_operation in &local.operations {
            push_create_api_operation(
                &mut commands.operation_creates,
                metadata,
                &integration_id,
                local_operation,
            );
        }
    }

    commands
}

#[derive(Default)]
struct SimpleLifecycleCommands {
    deletes: Vec<adk_protobuf::Command>,
    creates: Vec<adk_protobuf::Command>,
    updates: Vec<adk_protobuf::Command>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct KeyphraseItem {
    id: String,
    keyphrase: String,
    level: String,
}

fn keyphrase_lifecycle_commands(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> SimpleLifecycleCommands {
    let Some(yaml) = resource_yaml(
        resources,
        "voice/speech_recognition/keyphrase_boosting.yaml",
    )
    .or_else(|| resource_yaml(resources, "speech_recognition/keyphrase_boosting.yaml")) else {
        return SimpleLifecycleCommands::default();
    };
    let local_items = local_keyphrase_items(&yaml);
    let remote_items = remote_keyphrase_items(projection);
    let local_keyphrases = local_items
        .iter()
        .map(|item| item.keyphrase.clone())
        .collect::<HashSet<_>>();
    let remote_by_keyphrase = remote_items
        .iter()
        .map(|item| (item.keyphrase.clone(), item.clone()))
        .collect::<HashMap<_, _>>();

    let mut commands = SimpleLifecycleCommands::default();
    for remote in &remote_items {
        if !local_keyphrases.contains(&remote.keyphrase) {
            push_command(
                &mut commands.deletes,
                metadata,
                "delete_keyphrase_boosting",
                CommandPayload::DeleteKeyphraseBoosting(KeyphraseBoostingDeleteKeyphrase {
                    id: remote.id.clone(),
                }),
            );
        }
    }
    for local in &local_items {
        match remote_by_keyphrase.get(&local.keyphrase) {
            Some(remote) if local.level != remote.level => push_command(
                &mut commands.updates,
                metadata,
                "update_keyphrase_boosting",
                CommandPayload::UpdateKeyphraseBoosting(KeyphraseBoostingUpdateKeyphrase {
                    id: remote.id.clone(),
                    keyphrase: Some(local.keyphrase.clone()),
                    level: Some(local.level.clone()),
                }),
            ),
            Some(_) => {}
            None => {
                let id = generated_replay_resource_id(
                    "keyphrase_boosting",
                    &local.keyphrase,
                    "voice/speech_recognition/keyphrase_boosting.yaml",
                )
                .unwrap_or_else(|| random_resource_id("KEYPHRASE_BOOSTING"));
                push_command(
                    &mut commands.creates,
                    metadata,
                    "create_keyphrase_boosting",
                    CommandPayload::CreateKeyphraseBoosting(KeyphraseBoostingCreateKeyphrase {
                        id,
                        keyphrase: local.keyphrase.clone(),
                        level: local.level.clone(),
                    }),
                );
            }
        }
    }
    commands
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TranscriptItem {
    id: String,
    name: String,
    description: String,
    regular_expressions: Vec<RegularExpression>,
}

fn transcript_lifecycle_commands(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> SimpleLifecycleCommands {
    let Some(yaml) = resource_yaml(
        resources,
        "voice/speech_recognition/transcript_corrections.yaml",
    ) else {
        return SimpleLifecycleCommands::default();
    };
    let local_items = local_transcript_items(&yaml);
    let remote_items = remote_transcript_items(projection);
    let local_names = local_items
        .iter()
        .map(|item| item.name.clone())
        .collect::<HashSet<_>>();
    let remote_by_name = remote_items
        .iter()
        .map(|item| (item.name.clone(), item.clone()))
        .collect::<HashMap<_, _>>();

    let mut commands = SimpleLifecycleCommands::default();
    for remote in &remote_items {
        if !local_names.contains(&remote.name) {
            push_command(
                &mut commands.deletes,
                metadata,
                "delete_transcript_corrections",
                CommandPayload::DeleteTranscriptCorrections(
                    TranscriptCorrectionsDeleteTranscriptCorrections {
                        transcript_corrections_id: remote.id.clone(),
                    },
                ),
            );
        }
    }
    let mut updated_corrections = Vec::new();
    for local in &local_items {
        match remote_by_name.get(&local.name) {
            Some(remote) => {
                let merged = transcript_item_with_remote_regex_ids(local, remote);
                if &merged != remote {
                    updated_corrections.push(transcript_correction_proto(&merged));
                }
            }
            None => {
                let id = generated_replay_resource_id(
                    "transcript_corrections",
                    &local.name,
                    "voice/speech_recognition/transcript_corrections.yaml",
                )
                .unwrap_or_else(|| random_resource_id("TRANSCRIPT_CORRECTIONS"));
                push_command(
                    &mut commands.creates,
                    metadata,
                    "create_transcript_corrections",
                    CommandPayload::CreateTranscriptCorrections(
                        TranscriptCorrectionsCreateTranscriptCorrections {
                            id: id.clone(),
                            name: local.name.clone(),
                            description: Some(local.description.clone()),
                            regular_expressions: transcript_regexes_with_ids(local, &id),
                        },
                    ),
                );
            }
        }
    }
    if !updated_corrections.is_empty() {
        push_command(
            &mut commands.updates,
            metadata,
            "update_transcript_corrections",
            CommandPayload::UpdateTranscriptCorrections(
                TranscriptCorrectionsUpdateTranscriptCorrections {
                    data: Some(TranscriptCorrectionsUpdateData {
                        corrections: updated_corrections,
                    }),
                },
            ),
        );
    }
    commands
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PronunciationItem {
    id: String,
    regex: String,
    replacement: String,
    case_sensitive: bool,
    language_code: String,
    description: String,
    position: i32,
    name: String,
}

fn pronunciation_lifecycle_commands(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> SimpleLifecycleCommands {
    let Some(yaml) = resource_yaml(resources, "voice/response_control/pronunciations.yaml") else {
        return SimpleLifecycleCommands::default();
    };
    let local_items = local_pronunciation_items(&yaml);
    let remote_items = remote_pronunciation_items(projection);
    let local_positions = local_items
        .iter()
        .map(|item| item.position)
        .collect::<HashSet<_>>();
    let remote_by_position = remote_items
        .iter()
        .map(|item| (item.position, item.clone()))
        .collect::<HashMap<_, _>>();

    let mut commands = SimpleLifecycleCommands::default();
    for remote in &remote_items {
        if !local_positions.contains(&remote.position) {
            push_command(
                &mut commands.deletes,
                metadata,
                "pronunciations_delete_pronunciation",
                CommandPayload::PronunciationsDeletePronunciation(
                    PronunciationsDeletePronunciation {
                        id: remote.id.clone(),
                    },
                ),
            );
        }
    }
    for local in &local_items {
        match remote_by_position.get(&local.position) {
            Some(remote) if pronunciation_item_needs_update(local, remote) => push_command(
                &mut commands.updates,
                metadata,
                "pronunciations_update_pronunciation",
                CommandPayload::PronunciationsUpdatePronunciation(
                    PronunciationsUpdatePronunciation {
                        id: Some(remote.id.clone()),
                        regex: Some(local.regex.clone()),
                        replacement: Some(local.replacement.clone()),
                        case_sensitive: Some(local.case_sensitive),
                        language_code: Some(local.language_code.clone()),
                        description: Some(local.description.clone()),
                        position: Some(local.position),
                        name: Some(local.name.clone()),
                    },
                ),
            ),
            Some(_) => {}
            None => {
                let id = generated_replay_resource_id(
                    "pronunciations",
                    &local.regex,
                    "voice/response_control/pronunciations.yaml",
                )
                .unwrap_or_else(|| random_resource_id("PRONUNCIATIONS"));
                push_command(
                    &mut commands.creates,
                    metadata,
                    "pronunciations_create_pronunciation",
                    CommandPayload::PronunciationsCreatePronunciation(
                        PronunciationsCreatePronunciation {
                            id,
                            regex: local.regex.clone(),
                            replacement: local.replacement.clone(),
                            case_sensitive: local.case_sensitive,
                            language_code: local.language_code.clone(),
                            description: local.description.clone(),
                            position: local.position,
                            name: local.name.clone(),
                        },
                    ),
                );
            }
        }
    }
    commands
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
        CommandPayload::VariantDeleteVariant(msg) => Some((
            "variant_delete_variant",
            json!({
                "id": msg.id,
            }),
        )),
        CommandPayload::VariantDeleteAttribute(msg) => Some((
            "variant_delete_attribute",
            json!({
                "id": msg.id,
            }),
        )),
        CommandPayload::VariantSetDefaultVariant(msg) => Some((
            "variant_set_default_variant",
            json!({
                "id": msg.id,
            }),
        )),
        CommandPayload::VariantUpdateAttribute(msg) => {
            let mut value = serde_json::Map::new();
            value.insert("id".to_string(), json!(msg.id));
            if let Some(name) = &msg.name {
                value.insert("name".to_string(), json!(name));
            }
            if let Some(references) = &msg.references {
                value.insert(
                    "references".to_string(),
                    attribute_references_json(Some(references)),
                );
            }
            value.insert(
                "variant_values".to_string(),
                json!({
                    "values": msg
                        .variant_values
                        .as_ref()
                        .map(|values| json!(values.values))
                        .unwrap_or_else(|| json!({})),
                }),
            );
            Some(("variant_update_attribute", Value::Object(value)))
        }
        CommandPayload::CreateApiIntegration(msg) => Some((
            "create_api_integration",
            json!({
                "id": msg.id,
                "name": msg.name,
                "description": msg.description.clone().unwrap_or_default(),
                "environments": environments_json(msg.environments.as_ref()),
            }),
        )),
        CommandPayload::UpdateApiIntegration(msg) => Some((
            "update_api_integration",
            json!({
                "id": msg.id,
                "name": msg.name.clone().unwrap_or_default(),
                "description": msg.description.clone().unwrap_or_default(),
            }),
        )),
        CommandPayload::DeleteApiIntegration(msg) => Some((
            "delete_api_integration",
            json!({
                "id": msg.id,
            }),
        )),
        CommandPayload::UpdateApiIntegrationConfig(msg) => Some((
            "update_api_integration_config",
            json!({
                "id": msg.id,
                "environment": msg.environment,
                "base_url": msg.base_url.clone().unwrap_or_default(),
                "auth_type": msg.auth_type.clone().unwrap_or_default(),
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
        CommandPayload::UpdateApiIntegrationOperation(msg) => Some((
            "update_api_integration_operation",
            json!({
                "id": msg.id,
                "integration_id": msg.integration_id.clone().unwrap_or_default(),
                "name": msg.name.clone().unwrap_or_default(),
                "method": msg.method.clone().unwrap_or_default(),
                "resource": msg.resource.clone().unwrap_or_default(),
            }),
        )),
        CommandPayload::DeleteApiIntegrationOperation(msg) => Some((
            "delete_api_integration_operation",
            json!({
                "id": msg.id,
                "integration_id": msg.integration_id,
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
        CommandPayload::UpdateKeyphraseBoosting(msg) => Some((
            "update_keyphrase_boosting",
            json!({
                "id": msg.id,
                "keyphrase": msg.keyphrase.clone().unwrap_or_default(),
                "level": msg.level.clone().unwrap_or_default(),
            }),
        )),
        CommandPayload::DeleteKeyphraseBoosting(msg) => Some((
            "delete_keyphrase_boosting",
            json!({
                "id": msg.id,
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
        CommandPayload::UpdateTranscriptCorrections(msg) => Some((
            "update_transcript_corrections",
            json!({
                "data": {
                    "corrections": msg
                        .data
                        .as_ref()
                        .map(|data| {
                            data.corrections
                                .iter()
                                .map(transcript_correction_json)
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default(),
                },
            }),
        )),
        CommandPayload::DeleteTranscriptCorrections(msg) => Some((
            "delete_transcript_corrections",
            json!({
                "transcript_corrections_id": msg.transcript_corrections_id,
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
        CommandPayload::PronunciationsUpdatePronunciation(msg) => Some((
            "pronunciations_update_pronunciation",
            json!({
                "id": msg.id.clone().unwrap_or_default(),
                "regex": msg.regex.clone().unwrap_or_default(),
                "replacement": msg.replacement.clone().unwrap_or_default(),
                "case_sensitive": msg.case_sensitive.unwrap_or(false),
                "language_code": msg.language_code.clone().unwrap_or_default(),
                "description": msg.description.clone().unwrap_or_default(),
                "position": msg.position.unwrap_or(0),
                "name": msg.name.clone().unwrap_or_default(),
            }),
        )),
        CommandPayload::PronunciationsDeletePronunciation(msg) => Some((
            "pronunciations_delete_pronunciation",
            json!({
                "id": msg.id,
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

fn yaml_sequence<'a>(yaml: &'a serde_yaml::Value, key: &str) -> Vec<&'a serde_yaml::Value> {
    yaml.get(key)
        .and_then(serde_yaml::Value::as_sequence)
        .map(|items| items.iter().collect())
        .unwrap_or_default()
}

fn yaml_bool(yaml: &serde_yaml::Value, key: &str) -> bool {
    yaml.get(key)
        .and_then(serde_yaml::Value::as_bool)
        .unwrap_or(false)
}

fn projection_entities<'a>(root: &'a Value, path: &[&str]) -> Vec<(String, &'a Value)> {
    let mut current = root;
    for key in path {
        let Some(next) = current.get(*key) else {
            return Vec::new();
        };
        current = next;
    }
    let Some(entities) = current.get("entities").and_then(Value::as_object) else {
        return Vec::new();
    };

    let mut ordered = Vec::new();
    let mut seen = HashSet::new();
    if let Some(ids) = current.get("ids").and_then(Value::as_array) {
        for id in ids.iter().filter_map(Value::as_str) {
            if let Some(entity) = entities.get(id) {
                ordered.push((id.to_string(), entity));
                seen.insert(id.to_string());
            }
        }
    }

    let mut remaining = entities
        .iter()
        .filter(|(id, _)| !seen.contains(*id))
        .collect::<Vec<_>>();
    remaining.sort_by_key(|(left, _)| *left);
    ordered.extend(
        remaining
            .into_iter()
            .map(|(id, entity)| (id.clone(), entity)),
    );
    ordered
}

fn json_str(value: &Value, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .unwrap_or("")
        .to_string()
}

fn json_bool(value: &Value, keys: &[&str]) -> bool {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_bool))
        .unwrap_or(false)
}

fn json_i32(value: &Value, keys: &[&str]) -> i32 {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_i64))
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or(0)
}

fn local_variant_items(yaml: &serde_yaml::Value) -> Vec<VariantItem> {
    yaml_sequence(yaml, "variants")
        .into_iter()
        .filter_map(|item| {
            let name = yaml_str(item, "name");
            if name.is_empty() {
                return None;
            }
            Some(VariantItem {
                id: String::new(),
                name,
                is_default: yaml_bool(item, "is_default"),
            })
        })
        .collect()
}

fn remote_variant_items(projection: &Value) -> Vec<VariantItem> {
    projection_entities(projection, &["variantManagement", "variants"])
        .into_iter()
        .filter_map(|(id, value)| {
            let name = json_str(value, &["name"]);
            if name.is_empty() {
                return None;
            }
            Some(VariantItem {
                id,
                name,
                is_default: json_bool(value, &["isDefault", "is_default"]),
            })
        })
        .collect()
}

fn local_variant_attribute_items(yaml: &serde_yaml::Value) -> Vec<VariantAttributeItem> {
    yaml_sequence(yaml, "attributes")
        .into_iter()
        .filter_map(|item| {
            let name = yaml_str(item, "name");
            if name.is_empty() {
                return None;
            }
            Some(VariantAttributeItem {
                id: String::new(),
                name,
                values: yaml_string_map(item.get("values")),
            })
        })
        .collect()
}

fn remote_variant_attribute_items(projection: &Value) -> Vec<VariantAttributeItem> {
    let variants_by_id = remote_variant_items(projection)
        .into_iter()
        .map(|item| (item.id, item.name))
        .collect::<HashMap<_, _>>();
    let mut attributes = projection_entities(projection, &["variantManagement", "attributes"])
        .into_iter()
        .filter_map(|(id, value)| {
            if json_bool(value, &["archived"]) {
                return None;
            }
            let name = json_str(value, &["name"]);
            if name.is_empty() {
                return None;
            }
            Some(VariantAttributeItem {
                id,
                name,
                values: HashMap::new(),
            })
        })
        .collect::<Vec<_>>();
    let attribute_index_by_id = attributes
        .iter()
        .enumerate()
        .map(|(idx, item)| (item.id.clone(), idx))
        .collect::<HashMap<_, _>>();

    for (variant_id, value) in
        projection_entities(projection, &["variantManagement", "variantAttributeValues"])
    {
        if !variants_by_id.contains_key(&variant_id) {
            continue;
        }
        let Some(values) = value.get("values").and_then(Value::as_object) else {
            continue;
        };
        for (attribute_id, attribute_value) in values {
            let Some(index) = attribute_index_by_id.get(attribute_id).copied() else {
                continue;
            };
            attributes[index].values.insert(
                variant_id.clone(),
                attribute_value.as_str().unwrap_or("").to_string(),
            );
        }
    }
    attributes
}

fn variant_attribute_values_with_ids(
    values: &HashMap<String, String>,
    variant_ids_by_name: &HashMap<String, String>,
) -> HashMap<String, String> {
    values
        .iter()
        .filter_map(|(variant_name, value)| {
            Some((
                variant_ids_by_name.get(variant_name)?.clone(),
                value.clone(),
            ))
        })
        .collect()
}

fn empty_attribute_references() -> AttributeReferences {
    AttributeReferences {
        topics: HashMap::new(),
        flow_steps: HashMap::new(),
        no_code_steps: HashMap::new(),
    }
}

fn yaml_string_map(value: Option<&serde_yaml::Value>) -> HashMap<String, String> {
    value
        .and_then(serde_yaml::Value::as_mapping)
        .map(|items| {
            items
                .iter()
                .filter_map(|(key, value)| {
                    Some((key.as_str()?.to_string(), value.as_str()?.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn local_api_integration_items(yaml: &serde_yaml::Value) -> Vec<ApiIntegrationItem> {
    yaml_sequence(yaml, "api_integrations")
        .into_iter()
        .filter_map(|item| {
            let name = yaml_str(item, "name");
            if name.is_empty() {
                return None;
            }
            Some(ApiIntegrationItem {
                id: String::new(),
                name,
                description: yaml_str(item, "description"),
                environments: api_environment_items_from_yaml(item),
                operations: api_operations_from_yaml(item),
            })
        })
        .collect()
}

fn remote_api_integration_items(projection: &Value) -> Vec<ApiIntegrationItem> {
    projection_entities(projection, &["apiIntegrations", "apiIntegrations"])
        .into_iter()
        .filter_map(|(id, value)| {
            let name = json_str(value, &["name"]);
            if name.is_empty() {
                return None;
            }
            Some(ApiIntegrationItem {
                id,
                name,
                description: json_str(value, &["description"]),
                environments: api_environment_items_from_projection(value),
                operations: api_operations_from_projection(value),
            })
        })
        .collect()
}

fn api_environment_items_from_yaml(
    integration: &serde_yaml::Value,
) -> HashMap<String, ApiEnvironmentItem> {
    let Some(envs) = integration
        .get("environments")
        .and_then(serde_yaml::Value::as_mapping)
    else {
        return HashMap::new();
    };
    let mut out = HashMap::new();
    for (source_key, normalized_key) in [
        ("sandbox", "sandbox"),
        ("pre-release", "pre_release"),
        ("pre_release", "pre_release"),
        ("live", "live"),
    ] {
        if let Some(env) = envs.get(serde_yaml::Value::String(source_key.to_string())) {
            out.insert(
                normalized_key.to_string(),
                ApiEnvironmentItem {
                    base_url: yaml_str(env, "base_url"),
                    auth_type: yaml_str(env, "auth_type"),
                },
            );
        }
    }
    out
}

fn api_environment_items_from_projection(value: &Value) -> HashMap<String, ApiEnvironmentItem> {
    let Some(envs) = value.get("environments").and_then(Value::as_object) else {
        return HashMap::new();
    };
    let mut out = HashMap::new();
    for (source_key, normalized_key) in [
        ("sandbox", "sandbox"),
        ("pre-release", "pre_release"),
        ("preRelease", "pre_release"),
        ("pre_release", "pre_release"),
        ("live", "live"),
    ] {
        if let Some(env) = envs.get(source_key) {
            out.insert(
                normalized_key.to_string(),
                ApiEnvironmentItem {
                    base_url: json_str(env, &["baseUrl", "base_url"]),
                    auth_type: json_str(env, &["authType", "auth_type"]),
                },
            );
        }
    }
    out
}

fn api_operations_from_yaml(integration: &serde_yaml::Value) -> Vec<ApiOperationItem> {
    yaml_sequence(integration, "operations")
        .into_iter()
        .filter_map(|item| {
            let name = yaml_str(item, "name");
            if name.is_empty() {
                return None;
            }
            Some(ApiOperationItem {
                id: String::new(),
                name,
                method: yaml_str(item, "method"),
                resource: yaml_str(item, "resource"),
            })
        })
        .collect()
}

fn api_operations_from_projection(integration: &Value) -> Vec<ApiOperationItem> {
    let Some(operations) = integration.get("operations") else {
        return Vec::new();
    };
    let mut items = if let Some(entities) = operations.get("entities").and_then(Value::as_object) {
        let ids = operations.get("ids").and_then(Value::as_array);
        let mut ordered = Vec::new();
        let mut seen = HashSet::new();
        if let Some(ids) = ids {
            for id in ids.iter().filter_map(Value::as_str) {
                if let Some(operation) = entities.get(id) {
                    ordered.push((id.to_string(), operation));
                    seen.insert(id.to_string());
                }
            }
        }
        let mut remaining = entities
            .iter()
            .filter(|(id, _)| !seen.contains(*id))
            .collect::<Vec<_>>();
        remaining.sort_by_key(|(left, _)| *left);
        ordered.extend(
            remaining
                .into_iter()
                .map(|(id, operation)| (id.clone(), operation)),
        );
        ordered
    } else if let Some(object) = operations.as_object() {
        let mut pairs = object
            .iter()
            .map(|(id, operation)| (id.clone(), operation))
            .collect::<Vec<_>>();
        pairs.sort_by(|(left, _), (right, _)| left.cmp(right));
        pairs
    } else {
        Vec::new()
    };

    items
        .drain(..)
        .filter_map(|(id, value)| {
            let name = json_str(value, &["name"]);
            if name.is_empty() {
                return None;
            }
            Some(ApiOperationItem {
                id,
                name,
                method: json_str(value, &["method"]),
                resource: json_str(value, &["resource"]),
            })
        })
        .collect()
}

fn environments_from_items(items: &HashMap<String, ApiEnvironmentItem>) -> Environments {
    Environments {
        sandbox: items.get("sandbox").map(api_config_from_item),
        pre_release: items.get("pre_release").map(api_config_from_item),
        live: items.get("live").map(api_config_from_item),
    }
}

fn api_config_from_item(item: &ApiEnvironmentItem) -> ApiIntegrationConfig {
    ApiIntegrationConfig {
        base_url: item.base_url.clone(),
        auth_type: item.auth_type.clone(),
    }
}

fn push_create_api_operation(
    commands: &mut Vec<adk_protobuf::Command>,
    metadata: &Option<Metadata>,
    integration_id: &str,
    operation: &ApiOperationItem,
) {
    let id = generated_replay_resource_id(
        "api_integration_operation",
        &operation.name,
        "config/api_integrations.yaml",
    )
    .unwrap_or_else(|| Uuid::new_v4().to_string());
    push_command(
        commands,
        metadata,
        "create_api_integration_operation",
        CommandPayload::CreateApiIntegrationOperation(ApiIntegrationOperationCreate {
            name: operation.name.clone(),
            method: operation.method.clone(),
            resource: operation.resource.clone(),
            integration_id: integration_id.to_string(),
            id,
        }),
    );
}

fn local_keyphrase_items(yaml: &serde_yaml::Value) -> Vec<KeyphraseItem> {
    yaml_sequence(yaml, "keyphrases")
        .into_iter()
        .filter_map(|item| {
            let keyphrase = yaml_str(item, "keyphrase");
            if keyphrase.is_empty() {
                return None;
            }
            Some(KeyphraseItem {
                id: String::new(),
                keyphrase,
                level: yaml_str(item, "level"),
            })
        })
        .collect()
}

fn remote_keyphrase_items(projection: &Value) -> Vec<KeyphraseItem> {
    projection_entities(projection, &["keyphraseBoosting", "keyphraseBoosting"])
        .into_iter()
        .filter_map(|(id, value)| {
            let keyphrase = json_str(value, &["keyphrase"]);
            if keyphrase.is_empty() {
                return None;
            }
            Some(KeyphraseItem {
                id,
                keyphrase,
                level: json_str(value, &["level"]),
            })
        })
        .collect()
}

fn local_transcript_items(yaml: &serde_yaml::Value) -> Vec<TranscriptItem> {
    yaml_sequence(yaml, "corrections")
        .into_iter()
        .filter_map(|item| {
            let name = yaml_str(item, "name");
            if name.is_empty() {
                return None;
            }
            Some(TranscriptItem {
                id: String::new(),
                name,
                description: yaml_str(item, "description"),
                regular_expressions: regexes_from_yaml(item),
            })
        })
        .collect()
}

fn remote_transcript_items(projection: &Value) -> Vec<TranscriptItem> {
    projection_entities(
        projection,
        &["transcriptCorrections", "transcriptCorrections"],
    )
    .into_iter()
    .filter_map(|(id, value)| {
        let name = json_str(value, &["name"]);
        if name.is_empty() {
            return None;
        }
        Some(TranscriptItem {
            id,
            name,
            description: json_str(value, &["description"]),
            regular_expressions: regexes_from_projection(value),
        })
    })
    .collect()
}

fn regexes_from_yaml(item: &serde_yaml::Value) -> Vec<RegularExpression> {
    yaml_sequence(item, "regular_expressions")
        .into_iter()
        .map(|regex| RegularExpression {
            id: yaml_str(regex, "id"),
            regular_expression: yaml_str(regex, "regular_expression"),
            replacement: yaml_str(regex, "replacement"),
            replacement_type: yaml_str(regex, "replacement_type"),
        })
        .collect()
}

fn regexes_from_projection(item: &Value) -> Vec<RegularExpression> {
    item.get("regularExpressions")
        .or_else(|| item.get("regular_expressions"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|regex| RegularExpression {
            id: json_str(regex, &["id"]),
            regular_expression: json_str(regex, &["regularExpression", "regular_expression"]),
            replacement: json_str(regex, &["replacement"]),
            replacement_type: json_str(regex, &["replacementType", "replacement_type"]),
        })
        .collect()
}

fn transcript_item_with_remote_regex_ids(
    local: &TranscriptItem,
    remote: &TranscriptItem,
) -> TranscriptItem {
    let mut merged = local.clone();
    merged.id = remote.id.clone();
    for (idx, regex) in merged.regular_expressions.iter_mut().enumerate() {
        if regex.id.is_empty()
            && let Some(remote_id) = remote
                .regular_expressions
                .get(idx)
                .map(|regex| regex.id.clone())
                .filter(|id| !id.is_empty())
        {
            regex.id = remote_id;
        }
    }
    merged
}

fn transcript_regexes_with_ids(item: &TranscriptItem, id: &str) -> Vec<RegularExpression> {
    item.regular_expressions
        .iter()
        .enumerate()
        .map(|(idx, regex)| {
            let mut regex = regex.clone();
            if regex.id.is_empty() {
                regex.id = format!("{id}-REGEX-{idx}");
            }
            regex
        })
        .collect()
}

fn transcript_correction_proto(item: &TranscriptItem) -> TranscriptCorrection {
    TranscriptCorrection {
        id: item.id.clone(),
        name: item.name.clone(),
        description: item.description.clone(),
        regular_expressions: item.regular_expressions.clone(),
        created_by: String::new(),
        created_at: None,
        updated_by: String::new(),
        updated_at: None,
    }
}

fn local_pronunciation_items(yaml: &serde_yaml::Value) -> Vec<PronunciationItem> {
    yaml_sequence(yaml, "pronunciations")
        .into_iter()
        .enumerate()
        .filter_map(|(idx, item)| {
            let regex = yaml_str(item, "regex");
            if regex.is_empty() {
                return None;
            }
            Some(PronunciationItem {
                id: String::new(),
                regex,
                replacement: yaml_str(item, "replacement"),
                case_sensitive: yaml_bool(item, "case_sensitive"),
                language_code: yaml_str(item, "language_code"),
                description: yaml_str(item, "description"),
                position: item
                    .get("position")
                    .and_then(serde_yaml::Value::as_i64)
                    .and_then(|value| i32::try_from(value).ok())
                    .unwrap_or(idx as i32),
                name: yaml_str(item, "name"),
            })
        })
        .collect()
}

fn remote_pronunciation_items(projection: &Value) -> Vec<PronunciationItem> {
    projection_entities(projection, &["pronunciations", "pronunciations"])
        .into_iter()
        .filter_map(|(id, value)| {
            let regex = json_str(value, &["regex"]);
            if regex.is_empty() {
                return None;
            }
            Some(PronunciationItem {
                id,
                regex,
                replacement: json_str(value, &["replacement"]),
                case_sensitive: json_bool(value, &["caseSensitive", "case_sensitive"]),
                language_code: json_str(value, &["languageCode", "language_code"]),
                description: json_str(value, &["description"]),
                position: json_i32(value, &["position"]),
                name: json_str(value, &["name"]),
            })
        })
        .collect()
}

fn pronunciation_item_needs_update(local: &PronunciationItem, remote: &PronunciationItem) -> bool {
    local.regex != remote.regex
        || local.replacement != remote.replacement
        || local.case_sensitive != remote.case_sensitive
        || local.language_code != remote.language_code
        || local.description != remote.description
        || local.position != remote.position
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

fn transcript_correction_json(correction: &TranscriptCorrection) -> Value {
    json!({
        "id": correction.id,
        "name": correction.name,
        "description": correction.description,
        "regular_expressions": correction.regular_expressions.iter().map(regular_expression_json).collect::<Vec<_>>(),
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
