use adk_types::ResourceMap;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(super) struct SimpleLifecycleCommands {
    pub(super) deletes: Vec<adk_protobuf::Command>,
    pub(super) creates: Vec<adk_protobuf::Command>,
    pub(super) updates: Vec<adk_protobuf::Command>,
}

pub(super) fn resource_yaml(resources: &ResourceMap, path: &str) -> Option<serde_yaml::Value> {
    let content = resources.get(path)?.payload.get("content")?.as_str()?;
    serde_yaml::from_str(content).ok()
}

pub(super) fn first_yaml_mapping(value: &serde_yaml::Value) -> Option<&serde_yaml::Value> {
    value
        .as_sequence()?
        .iter()
        .find(|item| item.as_mapping().is_some())
}

pub(super) fn resource_changed(local: &ResourceMap, remote: &ResourceMap, path: &str) -> bool {
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

pub(super) fn yaml_sequence<'a>(
    yaml: &'a serde_yaml::Value,
    key: &str,
) -> Vec<&'a serde_yaml::Value> {
    yaml.get(key)
        .and_then(serde_yaml::Value::as_sequence)
        .map(|items| items.iter().collect())
        .unwrap_or_default()
}

pub(super) fn yaml_bool(yaml: &serde_yaml::Value, key: &str) -> bool {
    yaml.get(key)
        .and_then(serde_yaml::Value::as_bool)
        .unwrap_or(false)
}

pub(super) fn projection_entities<'a>(root: &'a Value, path: &[&str]) -> Vec<(String, &'a Value)> {
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

pub(super) fn json_str(value: &Value, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .unwrap_or("")
        .to_string()
}

pub(super) fn json_bool(value: &Value, keys: &[&str]) -> bool {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_bool))
        .unwrap_or(false)
}

pub(super) fn json_i32(value: &Value, keys: &[&str]) -> i32 {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_i64))
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or(0)
}

pub(super) fn yaml_string_map(value: Option<&serde_yaml::Value>) -> HashMap<String, String> {
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
