//! Platform projection to local resource materialization.

mod agent_settings;
mod broad_resources;
mod channels;
mod entities;
mod flows;
mod functions;
mod references;
mod synthetic;
mod topics;

pub(crate) use references::{
    FlowImportPathMaps, PromptReferenceMaps, flow_import_path_maps_from_projection,
    prompt_reference_maps_from_projection, replace_flow_import_ids_with_names,
    replace_flow_import_names_with_ids, replace_resource_names_with_ids,
    rewrite_materialized_prompt_references,
};

use crate::CommandGenError;
use crate::yaml_resources::to_yaml_string;
use adk_types::{Resource, ResourceMap};
use serde::Serialize;
use serde_json::Value;

// Define mapping from projection to resources in file system.
pub fn projection_to_resource_map(projection: &Value) -> Result<ResourceMap, CommandGenError> {
    let mut map = ResourceMap::new();
    let prompt_reference_maps = prompt_reference_maps_from_projection(projection);
    let flow_import_path_maps = flow_import_path_maps_from_projection(projection);

    topics::insert_topic_resources(&mut map, projection)?;
    functions::insert_function_resources(&mut map, projection, &flow_import_path_maps)?;
    flows::insert_flow_resources(&mut map, projection, &flow_import_path_maps)?;
    entities::insert_entity_resources(&mut map, projection)?;

    synthetic::insert_synthetic_resources(&mut map, projection)?;
    broad_resources::insert_broad_resources(&mut map, projection)?;
    agent_settings::insert_profile_and_safety_resources(&mut map, projection)?;
    channels::insert_channel_resources(&mut map, projection)?;
    agent_settings::insert_rules_resource(&mut map, projection)?;

    rewrite_materialized_prompt_references(&mut map, &prompt_reference_maps);
    Ok(map)
}

pub(super) fn projection_entities(root: &Value, path: &[&str]) -> Vec<(String, Value)> {
    let mut current = root;
    for key in path {
        let Some(next) = current.get(*key) else {
            return Vec::new();
        };
        current = next;
    }
    projection_entities_at(current)
}

pub(super) fn projection_nested_entities(root: &Value, path: &[&str]) -> Vec<(String, Value)> {
    let mut current = root;
    for key in path {
        let Some(next) = current.get(*key) else {
            return Vec::new();
        };
        current = next;
    }
    projection_entities_at(current)
}

pub(super) fn projection_entities_at(value: &Value) -> Vec<(String, Value)> {
    let Some(entities) = value.get("entities").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(ids) = value.get("ids").and_then(Value::as_array) {
        for id in ids.iter().filter_map(Value::as_str) {
            if let Some(entity) = entities.get(id) {
                out.push((id.to_string(), entity.clone()));
                seen.insert(id.to_string());
            }
        }
    }
    let mut remaining = entities
        .iter()
        .filter(|(id, _)| !seen.contains(*id))
        .collect::<Vec<_>>();
    remaining.sort_by_key(|(left, _)| *left);
    out.extend(
        remaining
            .into_iter()
            .map(|(id, entity)| (id.clone(), entity.clone())),
    );
    out
}

pub(super) fn insert_yaml_resource(
    map: &mut ResourceMap,
    file_path: &str,
    resource_id: &str,
    name: &str,
    value: impl Serialize,
) -> Result<(), CommandGenError> {
    let content =
        to_yaml_string(&value).map_err(|e| CommandGenError::InvalidData(e.to_string()))?;
    insert_content_resource(map, file_path, resource_id, name, content)
}

pub(super) fn insert_content_resource(
    map: &mut ResourceMap,
    file_path: &str,
    resource_id: &str,
    name: &str,
    content: String,
) -> Result<(), CommandGenError> {
    insert_resource(
        map,
        Resource {
            resource_id: resource_id.to_string(),
            name: name.to_string(),
            file_path: file_path.to_string(),
            payload: serde_json::json!({ "content": content }),
        },
    )
}

fn insert_resource(map: &mut ResourceMap, resource: Resource) -> Result<(), CommandGenError> {
    if map.contains_key(&resource.file_path) {
        return Err(CommandGenError::InvalidData(format!(
            "Duplicate resource file path found: {} for resource {}\nPlease rename the resource to avoid conflicts.",
            resource.file_path, resource.name
        )));
    }
    map.insert(resource.file_path.clone(), resource);
    Ok(())
}
