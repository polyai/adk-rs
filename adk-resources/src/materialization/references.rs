use super::flows::flow_entries;
use crate::clean_name;
use crate::projection::projection_entity_values;
use crate::specs::{AGENT_RULES_FILE, VARIANT_ATTRIBUTES};
use adk_types::ResourceMap;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub(crate) struct PromptReferenceMaps {
    global_id_to_name: HashMap<(String, String), String>,
    global_name_to_id: HashMap<(String, String), String>,
    flow_scoped_id_to_name: HashMap<String, HashMap<(String, String), String>>,
    flow_scoped_name_to_id: HashMap<String, HashMap<(String, String), String>>,
}

impl PromptReferenceMaps {
    fn insert_global(&mut self, prefix: &str, resource_id: &str, resource_name: &str) {
        if prefix.is_empty() || resource_id.is_empty() || resource_name.is_empty() {
            return;
        }
        self.global_id_to_name.insert(
            (prefix.to_string(), resource_id.to_string()),
            resource_name.to_string(),
        );
        self.global_name_to_id.insert(
            (prefix.to_string(), resource_name.to_string()),
            resource_id.to_string(),
        );
    }

    fn insert_flow_scoped(
        &mut self,
        flow_folder_name: &str,
        prefix: &str,
        resource_id: &str,
        resource_name: &str,
    ) {
        if flow_folder_name.is_empty()
            || prefix.is_empty()
            || resource_id.is_empty()
            || resource_name.is_empty()
        {
            return;
        }
        self.flow_scoped_id_to_name
            .entry(flow_folder_name.to_string())
            .or_default()
            .insert(
                (prefix.to_string(), resource_id.to_string()),
                resource_name.to_string(),
            );
        self.flow_scoped_name_to_id
            .entry(flow_folder_name.to_string())
            .or_default()
            .insert(
                (prefix.to_string(), resource_name.to_string()),
                resource_id.to_string(),
            );
    }

    fn id_to_name<'a>(
        &'a self,
        prefix: &str,
        value: &str,
        flow_folder_name: Option<&str>,
    ) -> Option<&'a str> {
        if let Some(name) = self
            .global_id_to_name
            .get(&(prefix.to_string(), value.to_string()))
        {
            return Some(name.as_str());
        }
        flow_folder_name.and_then(|flow_folder_name| {
            self.flow_scoped_id_to_name
                .get(flow_folder_name)
                .and_then(|map| map.get(&(prefix.to_string(), value.to_string())))
                .map(String::as_str)
        })
    }

    fn name_to_id<'a>(
        &'a self,
        prefix: &str,
        value: &str,
        flow_folder_name: Option<&str>,
    ) -> Option<&'a str> {
        if let Some(resource_id) = self
            .global_name_to_id
            .get(&(prefix.to_string(), value.to_string()))
        {
            return Some(resource_id.as_str());
        }
        flow_folder_name.and_then(|flow_folder_name| {
            self.flow_scoped_name_to_id
                .get(flow_folder_name)
                .and_then(|map| map.get(&(prefix.to_string(), value.to_string())))
                .map(String::as_str)
        })
    }
}

pub(crate) fn prompt_reference_maps_from_projection(projection: &Value) -> PromptReferenceMaps {
    let mut maps = PromptReferenceMaps::default();

    for (id, function) in projection_entity_values(projection, &["functions", "functions"]) {
        if function
            .get("archived")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let name = function
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id.as_str());
        maps.insert_global("fn", &id, name);
        if let Some(resource_id) = function.get("id").and_then(Value::as_str) {
            maps.insert_global("fn", resource_id, name);
        }
    }

    for (id, variable) in projection_entity_values(projection, &["variables", "variables"]) {
        if variable
            .get("archived")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let name = variable
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id.as_str());
        maps.insert_global("vrbl", &id, name);
        if let Some(resource_id) = variable.get("id").and_then(Value::as_str) {
            maps.insert_global("vrbl", resource_id, name);
        }
    }

    for (id, attribute) in VARIANT_ATTRIBUTES.owned_entries(projection) {
        if attribute
            .get("archived")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let name = attribute
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id.as_str());
        maps.insert_global("attr", &id, name);
        if let Some(resource_id) = attribute.get("id").and_then(Value::as_str) {
            maps.insert_global("attr", resource_id, name);
        }
    }

    for (flow_id, flow) in flow_entries(projection) {
        let flow_name = flow
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(flow_id.as_str());
        let flow_folder_name = clean_name(flow_name, true);
        for (transition_id, function) in projection_entity_values(&flow, &["transitionFunctions"])
            .into_iter()
            .chain(projection_entity_values(&flow, &["transition_functions"]))
        {
            if function
                .get("archived")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                continue;
            }
            let function_name = function
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(transition_id.as_str());
            maps.insert_flow_scoped(&flow_folder_name, "ft", &transition_id, function_name);
            if let Some(resource_id) = function.get("id").and_then(Value::as_str) {
                maps.insert_flow_scoped(&flow_folder_name, "ft", resource_id, function_name);
            }
        }
    }

    maps
}

pub(crate) fn replace_resource_ids_with_names(
    prompt: &str,
    prompt_reference_maps: &PromptReferenceMaps,
    flow_folder_name: Option<&str>,
) -> String {
    replace_prompt_references_with_lookup(prompt, |prefix, value| {
        prompt_reference_maps.id_to_name(prefix, value, flow_folder_name)
    })
}

pub(crate) fn replace_resource_names_with_ids(
    prompt: &str,
    prompt_reference_maps: &PromptReferenceMaps,
    flow_folder_name: Option<&str>,
) -> String {
    replace_prompt_references_with_lookup(prompt, |prefix, value| {
        prompt_reference_maps.name_to_id(prefix, value, flow_folder_name)
    })
}

fn replace_prompt_references_with_lookup<'a, F>(prompt: &str, mut lookup: F) -> String
where
    F: FnMut(&str, &str) -> Option<&'a str>,
{
    let mut out = String::with_capacity(prompt.len());
    let mut start = 0usize;
    while let Some(open_relative) = prompt[start..].find("{{") {
        let open = start + open_relative;
        out.push_str(&prompt[start..open]);
        let content_start = open + 2;
        let Some(close_relative) = prompt[content_start..].find("}}") else {
            out.push_str(&prompt[open..]);
            return out;
        };
        let close = content_start + close_relative;
        let reference = &prompt[content_start..close];
        if let Some((prefix, value)) = reference.split_once(':')
            && let Some(replacement) = lookup(prefix, value)
        {
            out.push_str("{{");
            out.push_str(prefix);
            out.push(':');
            out.push_str(replacement);
            out.push_str("}}");
        } else {
            out.push_str(&prompt[open..close + 2]);
        }
        start = close + 2;
    }
    out.push_str(&prompt[start..]);
    out
}

pub(crate) fn rewrite_materialized_prompt_references(
    map: &mut ResourceMap,
    prompt_reference_maps: &PromptReferenceMaps,
) {
    for resource in map.values_mut() {
        let file_path = resource.file_path.as_str();
        if !materialized_prompt_reference_file(file_path) {
            continue;
        }
        let Some(content) = resource
            .payload
            .get("content")
            .and_then(Value::as_str)
            .map(ToString::to_string)
        else {
            continue;
        };
        let flow_folder_name = flow_folder_name_for_path(file_path);
        let updated =
            replace_resource_ids_with_names(&content, prompt_reference_maps, flow_folder_name);
        if updated != content {
            resource.payload["content"] = Value::String(updated);
        }
    }
}

fn materialized_prompt_reference_file(file_path: &str) -> bool {
    file_path == AGENT_RULES_FILE.file_path
        || (file_path.starts_with("topics/") && file_path.ends_with(".yaml"))
        || (file_path.starts_with("flows/")
            && file_path.contains("/steps/")
            && file_path.ends_with(".yaml"))
}

fn flow_folder_name_for_path(file_path: &str) -> Option<&str> {
    let mut parts = file_path.split('/');
    if parts.next() == Some("flows") {
        parts.next()
    } else {
        None
    }
}

#[derive(Debug, Default)]
pub(crate) struct FlowImportPathMaps {
    id_to_flow_folder: HashMap<String, String>,
    flow_folder_to_id: HashMap<String, String>,
}

pub(crate) fn flow_import_path_maps_from_projection(projection: &Value) -> FlowImportPathMaps {
    let mut maps = FlowImportPathMaps::default();
    for (flow_key, flow) in flow_entries(projection) {
        let flow_id = flow
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or(flow_key.as_str());
        let flow_name = flow.get("name").and_then(Value::as_str).unwrap_or(flow_id);
        maps.insert(flow_id, flow_name);
        if flow_id != flow_key {
            maps.insert(flow_key.as_str(), flow_name);
        }
    }
    maps
}

impl FlowImportPathMaps {
    fn insert(&mut self, flow_id: &str, flow_name: &str) {
        let cleaned_id = clean_name(flow_id, true);
        let cleaned_name = clean_name(flow_name, true);
        if cleaned_id.is_empty() || cleaned_name.is_empty() {
            return;
        }
        self.id_to_flow_folder
            .insert(cleaned_id.clone(), cleaned_name.clone());
        self.flow_folder_to_id.insert(cleaned_name, cleaned_id);
    }
}

pub(crate) fn replace_flow_import_ids_with_names(
    code: &str,
    flow_import_path_maps: &FlowImportPathMaps,
) -> String {
    let mut normalized = code.to_string();
    for (flow_id, flow_folder) in &flow_import_path_maps.id_to_flow_folder {
        normalized = normalized.replace(
            &format!("functions.{flow_id}"),
            &format!("flows.{flow_folder}.functions"),
        );
    }
    normalized
}

pub(crate) fn replace_flow_import_names_with_ids(
    code: &str,
    flow_import_path_maps: &FlowImportPathMaps,
) -> String {
    let mut normalized = code.to_string();
    for (flow_folder, flow_id) in &flow_import_path_maps.flow_folder_to_id {
        normalized = normalized.replace(
            &format!("flows.{flow_folder}.functions"),
            &format!("functions.{flow_id}"),
        );
    }
    normalized
}
