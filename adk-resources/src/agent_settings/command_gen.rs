use crate::agent_settings::GeneralSafetyFilters;
use crate::agent_settings::discovery::{SettingsPersonality, SettingsRole};
use crate::local_parse::ParseLocalResource;
use crate::push_command_inputs::{resource_changed, resource_yaml};
use crate::specs::{
    AGENT_PERSONALITY_FILE, AGENT_ROLE_FILE, AGENT_RULES_FILE, AGENT_SAFETY_FILTERS_FILE,
};
use crate::{
    prompt_reference_maps_from_projection, push_command, replace_resource_names_with_ids,
    rules_references_from_behaviour, rules_references_from_projection,
};
use adk_protobuf::Metadata;
use adk_protobuf::agent::{
    Adjectives, PersonalityUpdatePersonality, RoleUpdateRole, RulesUpdateRules,
};
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::content_filter_settings::{
    AzureContentFilter, AzureContentFilterCategory,
    ContentFilterSettingsUpdateContentFilterSettings,
};
use adk_types::ResourceMap;
use serde_json::{self, Value as JsonValue, json};

#[cfg(test)]
#[path = "command_gen_tests.rs"]
mod command_gen_tests;

pub(crate) fn append_agent_settings_updates(
    commands: &mut Vec<adk_protobuf::Command>,
    resources: &ResourceMap,
    remote_resources: &ResourceMap,
    projection: &JsonValue,
    metadata: &Option<Metadata>,
) {
    append_rules_update(commands, resources, projection, metadata);
    append_personality_update(commands, resources, remote_resources, metadata);
    append_role_update(commands, resources, remote_resources, metadata);
    append_safety_filter_update(commands, resources, remote_resources, metadata);
}

fn append_rules_update(
    commands: &mut Vec<adk_protobuf::Command>,
    resources: &ResourceMap,
    projection: &JsonValue,
    metadata: &Option<Metadata>,
) {
    let Some(resource) = resources.get(AGENT_RULES_FILE.file_path) else {
        return;
    };
    let content = resource
        .payload
        .get("content")
        .and_then(JsonValue::as_str)
        .unwrap_or_default();
    let prompt_reference_maps = prompt_reference_maps_from_projection(projection);
    let normalized_content = replace_resource_names_with_ids(content, &prompt_reference_maps, None);
    let remote_behaviour = projection
        .pointer("/agentSettings/rules/behaviour")
        .and_then(JsonValue::as_str)
        .unwrap_or_default();
    if normalized_content == remote_behaviour {
        return;
    }
    push_command(
        commands,
        metadata,
        "update_rules",
        CommandPayload::UpdateRules(RulesUpdateRules {
            behaviour: Some(normalized_content.clone()),
            references: rules_references_from_behaviour(&normalized_content)
                .or_else(|| rules_references_from_projection(projection)),
        }),
    );
}

fn append_personality_update(
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
        && let Ok(personality) =
            SettingsPersonality::parse_local_yaml(AGENT_PERSONALITY_FILE.file_path, &yaml)
    {
        push_command(
            commands,
            metadata,
            "update_personality",
            CommandPayload::UpdatePersonality(PersonalityUpdatePersonality {
                adjectives: Some(Adjectives {
                    values: personality.allowed_adjective_values(),
                }),
                custom: Some(personality.custom().to_string()),
                references: None,
            }),
        );
    }
}

fn append_role_update(
    commands: &mut Vec<adk_protobuf::Command>,
    resources: &ResourceMap,
    remote_resources: &ResourceMap,
    metadata: &Option<Metadata>,
) {
    if resource_changed(resources, remote_resources, AGENT_ROLE_FILE.file_path)
        && let Some(yaml) = resource_yaml(resources, AGENT_ROLE_FILE.file_path)
        && let Ok(role) = SettingsRole::parse_local_yaml(AGENT_ROLE_FILE.file_path, &yaml)
    {
        push_command(
            commands,
            metadata,
            "update_role",
            CommandPayload::UpdateRole(RoleUpdateRole {
                value: Some(role.value().to_string()),
                additional_info: Some(role.additional_info().to_string()),
                custom: Some(role.custom().to_string()),
                references: None,
            }),
        );
    }
}

fn append_safety_filter_update(
    commands: &mut Vec<adk_protobuf::Command>,
    resources: &ResourceMap,
    remote_resources: &ResourceMap,
    metadata: &Option<Metadata>,
) {
    if resource_changed(
        resources,
        remote_resources,
        AGENT_SAFETY_FILTERS_FILE.file_path,
    ) && let Some(yaml) = resource_yaml(resources, AGENT_SAFETY_FILTERS_FILE.file_path)
        && let Ok(safety_filters) =
            GeneralSafetyFilters::parse_local_yaml(AGENT_SAFETY_FILTERS_FILE.file_path, &yaml)
    {
        push_command(
            commands,
            metadata,
            "update_content_filter_settings",
            CommandPayload::UpdateContentFilterSettings(safety_filters.to_update_proto()),
        );
    }
}

pub(crate) fn payload_json_summary(payload: &CommandPayload) -> Option<(&'static str, JsonValue)> {
    match payload {
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
        _ => None,
    }
}

fn content_filter_settings_json(
    settings: &ContentFilterSettingsUpdateContentFilterSettings,
) -> JsonValue {
    let mut value = serde_json::Map::new();
    value.insert(
        "type".to_string(),
        JsonValue::String(settings.r#type.clone().unwrap_or_default()),
    );
    value.insert(
        "disabled".to_string(),
        JsonValue::Bool(settings.disabled.unwrap_or(false)),
    );
    if let Some(azure_config) = &settings.azure_config {
        value.insert(
            "azure_config".to_string(),
            azure_content_filter_json(azure_config),
        );
    }
    JsonValue::Object(value)
}

fn azure_content_filter_json(filter: &AzureContentFilter) -> JsonValue {
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
    JsonValue::Object(value)
}

fn content_filter_category_json(category: &AzureContentFilterCategory) -> JsonValue {
    let mut value = serde_json::Map::new();
    if category.is_active {
        value.insert("is_active".to_string(), JsonValue::Bool(true));
    }
    value.insert(
        "precision".to_string(),
        JsonValue::String(category.precision.clone()),
    );
    JsonValue::Object(value)
}
