//! Shared input helpers for resource-family push-command builders.

use adk_types::ResourceMap;
use serde_json::Value as JsonValue;
use serde_yaml_ng::{Value as YamlValue, from_str};
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct SimpleLifecycleCommands {
    pub(crate) deletes: Vec<adk_protobuf::Command>,
    pub(crate) creates: Vec<adk_protobuf::Command>,
    pub(crate) updates: Vec<adk_protobuf::Command>,
}

pub(crate) fn resource_yaml(resources: &ResourceMap, path: &str) -> Option<YamlValue> {
    let content = resources.get(path)?.payload.get("content")?.as_str()?;
    from_str(content).ok()
}

pub(crate) fn first_yaml_mapping(value: &YamlValue) -> Option<&YamlValue> {
    value
        .as_sequence()?
        .iter()
        .find(|item| item.as_mapping().is_some())
}

pub(crate) fn resource_changed(local: &ResourceMap, remote: &ResourceMap, path: &str) -> bool {
    let Some(local_content) = local
        .get(path)
        .and_then(|resource| resource.payload.get("content"))
        .and_then(JsonValue::as_str)
    else {
        return false;
    };
    let Some(remote_content) = remote
        .get(path)
        .and_then(|resource| resource.payload.get("content"))
        .and_then(JsonValue::as_str)
    else {
        return true;
    };
    local_content != remote_content
}

pub(crate) fn yaml_sequence<'a>(yaml: &'a YamlValue, key: &str) -> Vec<&'a YamlValue> {
    yaml.get(key)
        .and_then(YamlValue::as_sequence)
        .map(|items| items.iter().collect())
        .unwrap_or_default()
}

pub(crate) fn yaml_bool(yaml: &YamlValue, key: &str) -> bool {
    yaml.get(key).and_then(YamlValue::as_bool).unwrap_or(false)
}

pub(crate) fn json_str(value: &JsonValue, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(JsonValue::as_str))
        .unwrap_or("")
        .to_string()
}

pub(crate) fn json_bool(value: &JsonValue, keys: &[&str]) -> bool {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(JsonValue::as_bool))
        .unwrap_or(false)
}

pub(crate) fn json_i32(value: &JsonValue, keys: &[&str]) -> i32 {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(JsonValue::as_i64))
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or(0)
}

pub(crate) fn yaml_string_map(value: Option<&YamlValue>) -> HashMap<String, String> {
    value
        .and_then(YamlValue::as_mapping)
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
