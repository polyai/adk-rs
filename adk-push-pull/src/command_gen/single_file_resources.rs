//! Push commands for resources whose local source of truth is one fixed file.
//!
//! This module owns both literal single-resource files (for example
//! `agent_settings/role.yaml`) and multi-entry files (for example
//! `config/api_integrations.yaml`). Directory and derived resources live in
//! `topics`, `functions`, `flows`, and `variables`.
//!
//! The private submodules below are just readability chunks within this resource
//! family; the top-level push ownership remains the five resource-family files.

#[path = "single_file_resources/interaction_files.rs"]
mod interaction_files;
#[path = "single_file_resources/structured.rs"]
mod structured;

use adk_protobuf::agent::RulesUpdateRules;
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::entities::{self, EntityCreate, EntityDelete, EntityUpdate};
use adk_protobuf::{Command, Metadata};
use adk_types::ResourceMap;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

use crate::{
    build_entity_create_config, build_entity_update_config, entity_entries,
    generated_replay_resource_id, is_synthetic_local_resource_id,
    prompt_reference_maps_from_projection, push_command, random_resource_id,
    replace_resource_names_with_ids, rules_references_from_behaviour,
    rules_references_from_projection, to_camel_case,
};

#[derive(Debug, Default)]
pub(crate) struct CommandGroups {
    pub deletes: Vec<Command>,
    pub creates: Vec<Command>,
    pub updates: Vec<Command>,
    pub post_updates: Vec<Command>,
}

impl CommandGroups {
    pub(crate) fn append(&mut self, other: CommandGroups) {
        self.deletes.extend(other.deletes);
        self.creates.extend(other.creates);
        self.updates.extend(other.updates);
        self.post_updates.extend(other.post_updates);
    }
}

pub(crate) fn single_file_resource_command_groups(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> CommandGroups {
    let mut groups = fixed_single_file_resource_command_groups(resources, projection, metadata);
    groups.append(interaction_files::interaction_file_resource_command_groups(
        resources, projection, metadata,
    ));
    groups.append(structured::structured_file_resource_command_groups(
        resources, projection, metadata,
    ));
    groups
}

fn fixed_single_file_resource_command_groups(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> CommandGroups {
    let prompt_reference_maps = prompt_reference_maps_from_projection(projection);
    let remote_entities = entity_entries(projection)
        .into_iter()
        .map(|(id, entity)| {
            let name = entity
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(id.as_str())
                .to_string();
            (name, (id, entity))
        })
        .collect::<HashMap<_, _>>();
    let mut local_entity_names = HashSet::new();
    let mut groups = CommandGroups::default();
    for resource in resources.values() {
        let path = resource.file_path.as_str();
        let content = resource
            .payload
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();

        if path == "agent_settings/rules.txt" {
            let normalized_content =
                replace_resource_names_with_ids(content, &prompt_reference_maps, None);
            let remote_behaviour = projection
                .pointer("/agentSettings/rules/behaviour")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if normalized_content != remote_behaviour {
                push_command(
                    &mut groups.updates,
                    metadata,
                    "update_rules",
                    CommandPayload::UpdateRules(RulesUpdateRules {
                        behaviour: Some(normalized_content.clone()),
                        references: rules_references_from_behaviour(&normalized_content)
                            .or_else(|| rules_references_from_projection(projection)),
                    }),
                );
            }
        } else if path == "config/entities.yaml"
            && let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(content)
            && let Some(items) = yaml
                .get("entities")
                .and_then(serde_yaml::Value::as_sequence)
        {
            for item in items {
                let name = item
                    .get("name")
                    .and_then(serde_yaml::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if name.is_empty() {
                    continue;
                }
                local_entity_names.insert(name.clone());
                let remote = remote_entities.get(&name);
                let id = remote
                    .map(|(id, _)| id.clone())
                    .or_else(|| {
                        (!is_synthetic_local_resource_id(&resource.resource_id))
                            .then_some(resource.resource_id.clone())
                    })
                    .unwrap_or_else(|| {
                        generated_replay_resource_id("entity", &name, "config/entities.yaml")
                            .unwrap_or_else(|| random_resource_id("ENTITIES"))
                    });
                let entity_type = item
                    .get("entity_type")
                    .and_then(serde_yaml::Value::as_str)
                    .unwrap_or("free_text");
                let description = item
                    .get("description")
                    .and_then(serde_yaml::Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let config = item.get("config");
                if let Some((_, remote_entity)) = remote {
                    if entity_item_matches_remote(item, remote_entity) {
                        continue;
                    }
                    push_command(
                        &mut groups.updates,
                        metadata,
                        "entity_update",
                        CommandPayload::EntityUpdate(EntityUpdate {
                            id: id.clone(),
                            name: name.clone(),
                            r#type: to_camel_case(entity_type),
                            description: description.clone(),
                            references: None,
                            config: build_entity_update_config(entity_type, config),
                        }),
                    );
                } else {
                    push_command(
                        &mut groups.creates,
                        metadata,
                        "entity_create",
                        CommandPayload::EntityCreate(EntityCreate {
                            id: id.clone(),
                            name: name.clone(),
                            r#type: to_camel_case(entity_type),
                            description: description.clone(),
                            references: None,
                            config: build_entity_create_config(entity_type, config),
                        }),
                    );
                }
            }
        }
    }

    for (name, (id, _)) in remote_entities {
        if !local_entity_names.contains(&name) {
            push_command(
                &mut groups.deletes,
                metadata,
                "entity_delete",
                CommandPayload::EntityDelete(EntityDelete { id }),
            );
        }
    }

    groups
}

fn entity_item_matches_remote(item: &serde_yaml::Value, remote: &Value) -> bool {
    let local_name = item
        .get("name")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or_default();
    let remote_name = remote
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if local_name != remote_name {
        return false;
    }

    let local_description = item
        .get("description")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or_default();
    let remote_description = remote
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if local_description != remote_description {
        return false;
    }

    let local_type = normalize_entity_type(
        item.get("entity_type")
            .and_then(serde_yaml::Value::as_str)
            .unwrap_or("free_text"),
    );
    let remote_type =
        normalize_entity_type(remote.get("type").and_then(Value::as_str).unwrap_or(""));
    if local_type != remote_type {
        return false;
    }

    let local_config = build_entity_update_config(&local_type, item.get("config"));
    let remote_config_yaml = remote_entity_config_yaml(remote);
    let remote_config = build_entity_update_config(&remote_type, Some(&remote_config_yaml));
    entity_update_config_json(local_config.as_ref())
        == entity_update_config_json(remote_config.as_ref())
}

fn remote_entity_config_yaml(remote: &Value) -> serde_yaml::Value {
    let config = remote
        .pointer("/config/value")
        .or_else(|| remote.get("config"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    serde_yaml::to_value(config).unwrap_or(serde_yaml::Value::Mapping(Default::default()))
}

fn normalize_entity_type(value: &str) -> String {
    let mut out = String::new();
    for (index, ch) in value.chars().enumerate() {
        if ch == '-' {
            out.push('_');
        } else if ch.is_ascii_uppercase() {
            if index > 0 && !out.ends_with('_') {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

pub(crate) fn payload_json_summary(payload: &CommandPayload) -> Option<(&'static str, Value)> {
    fixed_payload_json_summary(payload)
        .or_else(|| interaction_files::payload_json_summary(payload))
        .or_else(|| structured::payload_json_summary(payload))
}

fn fixed_payload_json_summary(payload: &CommandPayload) -> Option<(&'static str, Value)> {
    match payload {
        CommandPayload::EntityDelete(delete) => Some((
            "entity_delete",
            serde_json::json!({
                "id": delete.id,
            }),
        )),
        CommandPayload::EntityCreate(create) => {
            let mut value = serde_json::Map::new();
            value.insert("id".to_string(), Value::String(create.id.clone()));
            value.insert("name".to_string(), Value::String(create.name.clone()));
            value.insert("type".to_string(), Value::String(create.r#type.clone()));
            value.insert(
                "description".to_string(),
                Value::String(create.description.clone()),
            );
            value.insert(
                "references".to_string(),
                Value::Object(serde_json::Map::new()),
            );
            if let Some((key, config)) = entity_create_config_json(create.config.as_ref()) {
                value.insert(key.to_string(), config);
            }
            Some(("entity_create", Value::Object(value)))
        }
        CommandPayload::EntityUpdate(update) => {
            let mut value = serde_json::Map::new();
            value.insert("id".to_string(), Value::String(update.id.clone()));
            value.insert("name".to_string(), Value::String(update.name.clone()));
            value.insert("type".to_string(), Value::String(update.r#type.clone()));
            value.insert(
                "description".to_string(),
                Value::String(update.description.clone()),
            );
            if let Some((key, config)) = entity_update_config_json(update.config.as_ref()) {
                value.insert(key.to_string(), config);
            }
            Some(("entity_update", Value::Object(value)))
        }
        _ => None,
    }
}

fn entity_create_config_json(
    config: Option<&entities::entity_create::Config>,
) -> Option<(&'static str, Value)> {
    match config? {
        entities::entity_create::Config::Numeric(config) => {
            Some(("numeric", number_config_json(config)))
        }
        entities::entity_create::Config::Alphanumeric(config) => Some((
            "alphanumeric",
            serde_json::json!({
                "enabled": config.enabled,
                "validation_type": config.validation_type,
                "regular_expression": config.regular_expression,
            }),
        )),
        entities::entity_create::Config::Enum(config) => {
            Some(("enum", serde_json::json!({ "options": config.options })))
        }
        entities::entity_create::Config::Date(config) => Some((
            "date",
            serde_json::json!({ "relative_date": config.relative_date }),
        )),
        entities::entity_create::Config::PhoneNumber(config) => Some((
            "phone_number",
            serde_json::json!({
                "enabled": config.enabled,
                "country_codes": config.country_codes,
            }),
        )),
        entities::entity_create::Config::Time(config) => Some((
            "time",
            serde_json::json!({
                "enabled": config.enabled,
                "start_time": config.start_time,
                "end_time": config.end_time,
            }),
        )),
        entities::entity_create::Config::Address(_) => Some(("address", serde_json::json!({}))),
        entities::entity_create::Config::FreeText(_) => Some(("free_text", serde_json::json!({}))),
        entities::entity_create::Config::NameConfig(_) => {
            Some(("name_config", serde_json::json!({})))
        }
    }
}

fn entity_update_config_json(
    config: Option<&entities::entity_update::Config>,
) -> Option<(&'static str, Value)> {
    match config? {
        entities::entity_update::Config::Numeric(config) => {
            Some(("numeric", number_config_json(config)))
        }
        entities::entity_update::Config::Alphanumeric(config) => Some((
            "alphanumeric",
            serde_json::json!({
                "enabled": config.enabled,
                "validation_type": config.validation_type,
                "regular_expression": config.regular_expression,
            }),
        )),
        entities::entity_update::Config::Enum(config) => {
            Some(("enum", serde_json::json!({ "options": config.options })))
        }
        entities::entity_update::Config::Date(config) => Some((
            "date",
            serde_json::json!({ "relative_date": config.relative_date }),
        )),
        entities::entity_update::Config::PhoneNumber(config) => Some((
            "phone_number",
            serde_json::json!({
                "enabled": config.enabled,
                "country_codes": config.country_codes,
            }),
        )),
        entities::entity_update::Config::Time(config) => Some((
            "time",
            serde_json::json!({
                "enabled": config.enabled,
                "start_time": config.start_time,
                "end_time": config.end_time,
            }),
        )),
        entities::entity_update::Config::Address(_) => Some(("address", serde_json::json!({}))),
        entities::entity_update::Config::FreeText(_) => Some(("free_text", serde_json::json!({}))),
        entities::entity_update::Config::NameConfig(_) => {
            Some(("name_config", serde_json::json!({})))
        }
    }
}

fn number_config_json(config: &entities::NumberConfig) -> Value {
    let mut value = serde_json::Map::new();
    if config.has_decimal {
        value.insert("has_decimal".to_string(), Value::Bool(true));
    }
    if config.has_range {
        value.insert("has_range".to_string(), Value::Bool(true));
    }
    if let Some(min) = config.min {
        value.insert("min".to_string(), serde_json::json!(min));
    }
    if let Some(max) = config.max {
        value.insert("max".to_string(), serde_json::json!(max));
    }
    Value::Object(value)
}
