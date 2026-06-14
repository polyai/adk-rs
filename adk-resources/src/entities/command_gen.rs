//! Push commands for entity aggregate files.
//!
//! Entities are stored together in `config/entities.yaml`, but their command
//! generation semantics are specific to the entity resource family.

use serde_json::{self, Value as JsonValue};
use serde_yaml_ng::{Value as YamlValue, from_str};

use crate::push_commands::CommandGroups;
use adk_protobuf::Metadata;
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::entities::{self, EntityCreate, EntityDelete, EntityReferences, EntityUpdate};
use adk_types::ResourceMap;
use std::collections::{HashMap, HashSet};

use crate::entities::local::{
    ENTITIES_FILE_PATH, EntityItem as LocalEntityItem, parse_entities_content,
};
use crate::ids::stable_resource_id;
use crate::specs::{ENTITIES_FILE, ENTITY_ID_PREFIX};
use crate::{extract_entities_map, is_synthetic_local_resource_id, push_command};

pub(crate) fn entity_command_groups(
    resources: &ResourceMap,
    projection: &JsonValue,
    metadata: &Option<Metadata>,
) -> CommandGroups {
    let remote_entities = entity_entries(projection)
        .into_iter()
        .map(|(id, entity)| {
            let name = entity
                .get("name")
                .and_then(JsonValue::as_str)
                .unwrap_or(id.as_str())
                .to_string();
            (name, (id, entity))
        })
        .collect::<HashMap<_, _>>();
    let local_entities = local_entity_resources(resources, &remote_entities);
    let local_entity_ids = local_entities
        .iter()
        .map(|entity| (entity.item.name().to_string(), entity.id.clone()))
        .collect::<HashMap<_, _>>();
    let local_entity_references = local_entity_references(resources, projection, &local_entity_ids);
    let mut local_entity_names = HashSet::new();
    let mut groups = CommandGroups::default();
    for local in &local_entities {
        let name = local.item.name().to_string();
        local_entity_names.insert(name.clone());
        if let Some((remote_id, remote_entity)) = remote_entities.get(&name) {
            if entity_item_matches_remote(&local.item, remote_id, remote_entity) {
                continue;
            }
            push_command(
                &mut groups.updates,
                metadata,
                "entity_update",
                CommandPayload::EntityUpdate(EntityUpdate {
                    id: local.id.clone(),
                    name: name.clone(),
                    r#type: local.item.proto_type(),
                    description: local.item.description().to_string(),
                    references: None,
                    config: Some(local.item.update_config()),
                }),
            );
        } else {
            push_command(
                &mut groups.creates,
                metadata,
                "entity_create",
                CommandPayload::EntityCreate(EntityCreate {
                    id: local.id.clone(),
                    name: name.clone(),
                    r#type: local.item.proto_type(),
                    description: local.item.description().to_string(),
                    references: Some(
                        local_entity_references
                            .get(&local.id)
                            .cloned()
                            .unwrap_or_default(),
                    ),
                    config: Some(local.item.create_config()),
                }),
            );
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

fn entity_entries(projection: &JsonValue) -> HashMap<String, JsonValue> {
    extract_entities_map(projection, &["entities", "entities", "entities"])
}

struct LocalEntityResource {
    id: String,
    item: LocalEntityItem,
}

fn local_entity_resources(
    resources: &ResourceMap,
    remote_entities: &HashMap<String, (String, JsonValue)>,
) -> Vec<LocalEntityResource> {
    let mut entities = Vec::new();
    for resource in resources.values() {
        let path = resource.file_path.as_str();
        let content = resource
            .payload
            .get("content")
            .and_then(JsonValue::as_str)
            .unwrap_or_default();
        let Ok(items) = parse_entities_content(path, content) else {
            continue;
        };
        let resource_id = if path == ENTITIES_FILE_PATH {
            "local"
        } else {
            resource.resource_id.as_str()
        };
        for item in items {
            let id = local_entity_id(resource_id, item.name(), remote_entities.get(item.name()));
            entities.push(LocalEntityResource { id, item });
        }
    }
    entities
}

fn local_entity_id(resource_id: &str, name: &str, remote: Option<&(String, JsonValue)>) -> String {
    remote
        .map(|(id, _)| id.clone())
        .or_else(|| {
            (!is_synthetic_local_resource_id(resource_id)).then_some(resource_id.to_string())
        })
        .unwrap_or_else(|| stable_resource_id(ENTITY_ID_PREFIX, name, ENTITIES_FILE.file_path))
}

fn local_entity_references(
    resources: &ResourceMap,
    projection: &JsonValue,
    local_entity_ids: &HashMap<String, String>,
) -> HashMap<String, EntityReferences> {
    let flow_names = local_flow_names_by_folder(resources);
    let remote_step_ids = remote_step_ids_by_flow_and_name(projection);
    let mut references = HashMap::new();

    for resource in resources.values() {
        let path = resource.file_path.as_str();
        if !path.starts_with("flows/") || !path.contains("/steps/") || !path.ends_with(".yaml") {
            continue;
        }
        let content = resource
            .payload
            .get("content")
            .and_then(JsonValue::as_str)
            .unwrap_or_default();
        let Ok(yaml) = from_str::<YamlValue>(content) else {
            continue;
        };
        let step_name = yaml
            .get("name")
            .and_then(YamlValue::as_str)
            .filter(|name| !name.is_empty())
            .unwrap_or(&resource.name);
        let step_id = flow_folder_from_path(path)
            .and_then(|folder| {
                let flow_name = flow_names
                    .get(&folder)
                    .map_or(folder.as_str(), String::as_str);
                remote_step_ids
                    .get(&(flow_name.to_string(), step_name.to_string()))
                    .cloned()
            })
            .unwrap_or_else(|| stable_resource_id("FLOW_STEPS", step_name, path));
        let is_default_step = yaml
            .get("step_type")
            .and_then(YamlValue::as_str)
            .unwrap_or("advanced_step")
            == "default_step";

        let mut referenced_entities =
            extract_prompt_entity_references(yaml_str_value(&yaml, "prompt").as_str());
        referenced_entities.extend(yaml_value_string_list(yaml.get("extracted_entities")));
        referenced_entities.extend(condition_required_entities(&yaml));

        for entity in referenced_entities {
            let entity_id = resolve_entity_reference(&entity, local_entity_ids);
            let entry = references
                .entry(entity_id)
                .or_insert_with(EntityReferences::default);
            if is_default_step {
                entry.no_code_steps.insert(step_id.clone(), true);
            } else {
                entry.flow_steps.insert(step_id.clone(), true);
            }
        }
    }

    references
}

fn local_flow_names_by_folder(resources: &ResourceMap) -> HashMap<String, String> {
    let mut names = HashMap::new();
    for resource in resources.values() {
        let path = resource.file_path.as_str();
        if !path.starts_with("flows/") || !path.ends_with("/flow_config.yaml") {
            continue;
        }
        let Some(folder) = flow_folder_from_path(path) else {
            continue;
        };
        let content = resource
            .payload
            .get("content")
            .and_then(JsonValue::as_str)
            .unwrap_or_default();
        let Ok(yaml) = from_str::<YamlValue>(content) else {
            continue;
        };
        let name = yaml_str_value(&yaml, "name");
        if name.is_empty() {
            names.insert(folder.clone(), folder);
        } else {
            names.insert(folder, name);
        }
    }
    names
}

fn remote_step_ids_by_flow_and_name(projection: &JsonValue) -> HashMap<(String, String), String> {
    let mut ids = HashMap::new();
    let Some(flows) = projection
        .get("flows")
        .and_then(|flows| flows.get("flows"))
        .and_then(|flows| flows.get("entities"))
        .and_then(JsonValue::as_object)
    else {
        return ids;
    };

    for (flow_id, flow) in flows {
        let flow_name = flow
            .get("name")
            .and_then(JsonValue::as_str)
            .unwrap_or(flow_id.as_str())
            .to_string();
        let Some(steps) = flow
            .get("steps")
            .and_then(|steps| steps.get("entities"))
            .and_then(JsonValue::as_object)
        else {
            continue;
        };
        for (step_key, step) in steps {
            let Some(step_name) = step.get("name").and_then(JsonValue::as_str) else {
                continue;
            };
            let step_id = step
                .get("id")
                .and_then(JsonValue::as_str)
                .unwrap_or(step_key.as_str())
                .to_string();
            ids.insert((flow_name.clone(), step_name.to_string()), step_id);
        }
    }

    ids
}

fn flow_folder_from_path(path: &str) -> Option<String> {
    let mut parts = path.split('/');
    (parts.next()? == "flows").then_some(parts.next()?.to_string())
}

fn extract_prompt_entity_references(prompt: &str) -> Vec<String> {
    extract_template_references(prompt, "entity")
}

fn extract_template_references(prompt: &str, prefix: &str) -> Vec<String> {
    let marker = format!("{{{{{prefix}:");
    let mut out = Vec::new();
    let mut start = 0;
    while let Some(index) = prompt[start..].find(&marker) {
        let value_start = start + index + marker.len();
        let tail = &prompt[value_start..];
        let Some(end) = tail.find("}}") else {
            break;
        };
        let value = tail[..end].trim();
        if !value.is_empty() {
            out.push(value.to_string());
        }
        start = value_start + end + 2;
    }
    out
}

fn condition_required_entities(yaml: &YamlValue) -> Vec<String> {
    yaml.get("conditions")
        .and_then(YamlValue::as_sequence)
        .into_iter()
        .flatten()
        .flat_map(|condition| yaml_value_string_list(condition.get("required_entities")))
        .collect()
}

fn resolve_entity_reference(value: &str, local_entity_ids: &HashMap<String, String>) -> String {
    local_entity_ids
        .get(value)
        .cloned()
        .unwrap_or_else(|| value.to_string())
}

fn entity_item_matches_remote(item: &LocalEntityItem, remote_id: &str, remote: &JsonValue) -> bool {
    let Ok(Some(remote_item)) = LocalEntityItem::from_projection(remote_id, remote) else {
        return false;
    };
    item.name() == remote_item.name()
        && item.description() == remote_item.description()
        && item.entity_type() == remote_item.entity_type()
        && entity_update_config_json(Some(&item.update_config()))
            == entity_update_config_json(Some(&remote_item.update_config()))
}

pub(crate) fn payload_json_summary(payload: &CommandPayload) -> Option<(&'static str, JsonValue)> {
    match payload {
        CommandPayload::EntityDelete(delete) => Some((
            "entity_delete",
            serde_json::json!({
                "id": delete.id,
            }),
        )),
        CommandPayload::EntityCreate(create) => {
            let mut value = serde_json::Map::new();
            value.insert("id".to_string(), JsonValue::String(create.id.clone()));
            value.insert("name".to_string(), JsonValue::String(create.name.clone()));
            value.insert("type".to_string(), JsonValue::String(create.r#type.clone()));
            value.insert(
                "description".to_string(),
                JsonValue::String(create.description.clone()),
            );
            value.insert(
                "references".to_string(),
                entity_references_json(create.references.as_ref()),
            );
            if let Some((key, config)) = entity_create_config_json(create.config.as_ref()) {
                value.insert(key.to_string(), config);
            }
            Some(("entity_create", JsonValue::Object(value)))
        }
        CommandPayload::EntityUpdate(update) => {
            let mut value = serde_json::Map::new();
            value.insert("id".to_string(), JsonValue::String(update.id.clone()));
            value.insert("name".to_string(), JsonValue::String(update.name.clone()));
            value.insert("type".to_string(), JsonValue::String(update.r#type.clone()));
            value.insert(
                "description".to_string(),
                JsonValue::String(update.description.clone()),
            );
            if let Some((key, config)) = entity_update_config_json(update.config.as_ref()) {
                value.insert(key.to_string(), config);
            }
            Some(("entity_update", JsonValue::Object(value)))
        }
        _ => None,
    }
}

fn yaml_value_string_list(value: Option<&YamlValue>) -> Vec<String> {
    value
        .and_then(YamlValue::as_sequence)
        .map(|seq| {
            seq.iter()
                .filter_map(YamlValue::as_str)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn yaml_str_value(value: &YamlValue, key: &str) -> String {
    value
        .get(key)
        .and_then(YamlValue::as_str)
        .unwrap_or_default()
        .to_string()
}

fn entity_create_config_json(
    config: Option<&entities::entity_create::Config>,
) -> Option<(&'static str, JsonValue)> {
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
) -> Option<(&'static str, JsonValue)> {
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

fn number_config_json(config: &entities::NumberConfig) -> JsonValue {
    let mut value = serde_json::Map::new();
    if config.has_decimal {
        value.insert("has_decimal".to_string(), JsonValue::Bool(true));
    }
    if config.has_range {
        value.insert("has_range".to_string(), JsonValue::Bool(true));
    }
    if let Some(min) = config.min {
        value.insert("min".to_string(), serde_json::json!(min));
    }
    if let Some(max) = config.max {
        value.insert("max".to_string(), serde_json::json!(max));
    }
    JsonValue::Object(value)
}

fn entity_references_json(references: Option<&EntityReferences>) -> JsonValue {
    let Some(references) = references else {
        return JsonValue::Object(serde_json::Map::new());
    };
    if references.flow_steps.is_empty() && references.no_code_steps.is_empty() {
        return JsonValue::Object(serde_json::Map::new());
    }
    serde_json::json!({
        "flow_steps": references.flow_steps,
        "no_code_steps": references.no_code_steps,
    })
}

#[cfg(test)]
#[path = "command_gen_tests.rs"]
mod command_gen_tests;
