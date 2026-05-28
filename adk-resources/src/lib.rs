//! Resource-family semantics shared by core workflows and push/pull orchestration.
//!
//! This crate owns resource-specific behavior: discovery, local file layout,
//! projection paths, materialization, validation helpers, stable IDs, and
//! command generation helpers.

use adk_protobuf::agent::RulesReferences;
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CommandGenError {
    #[error("{0}")]
    InvalidData(String),
    #[error("{path}: Python syntax error while extracting ADK decorators: {message}")]
    PythonSyntax { path: String, message: String },
}

mod agent_settings;
mod api_integrations;
mod asr_settings;
mod channels;
pub mod discover;
mod entities;
mod experimental_config;
pub mod flows;
pub mod functions;
mod handoffs;
pub mod ids;
mod keyphrase_boosting;
mod local_resources;
mod materialization;
mod phrase_filters;
pub mod projection;
mod pronunciations;
mod push_commands;
pub mod resource_utils;
mod sms_templates;
pub mod specs;
pub mod status_snapshot;
#[cfg(test)]
mod test_support;
mod topics;
mod transcript_corrections;
mod variables;
mod variants;
mod yaml_resources;

pub use discover::{DISCOVER_DISPATCH, DiscoverDispatchEntry, discover_local_resources};
pub use flows::validate_flow_resources;
pub use functions::{
    FunctionValidationFailure, PYTHON_FLOW_IMPORT_STATUS_KEY_PREFIX,
    PYTHON_FUNCTION_STATUS_HASH_PREFIX, function_parameter_decorator_names,
    function_signature_parameter_list, function_signature_parameters, is_python_function_resource,
    legacy_python_function_raw, legacy_python_snapshot_hashes, local_resource_content,
    normalize_legacy_python_status_function_resources, normalize_python_function_metadata_spacing,
    resource_file_content, validate_flow_scoped_function_resource,
    validate_python_function_resource, validate_python_function_resources,
};
pub use local_resources::{
    DiscoveredResourceChanges, DiscoveredResourcePaths, TypedResourceLifecycle,
    build_typed_resource_lifecycle, empty_discovered_resource_paths, find_new_kept_deleted,
    type_name_to_resource_prefix, validate_semantic_resource,
};
pub use push_commands::{
    build_push_commands, build_push_commands_for_changed_resources, build_push_commands_with_actor,
    command_to_json_summary, try_build_push_commands,
    try_build_push_commands_for_changed_resources, try_build_push_commands_with_actor,
};
pub use status_snapshot::{
    FileStructureEntry, ResourceStatusPayloadInput, StatusResourcePayload, StatusSnapshot,
    current_status_hash_for_expected, flow_folder_name, legacy_python_rules_reference_names,
    legacy_python_status_resource_content, legacy_python_status_resource_file_hash,
    legacy_python_status_resource_path, python_json_dumps_pretty_sorted, python_json_dumps_sorted,
    resource_status_file_hash, resource_status_payload, status_flow_config_payload,
    status_flow_step_payload, status_function_payload, status_function_step_payload,
    status_pronunciation_hash_payload, status_pronunciation_payload, status_safety_filters_payload,
    status_variant_attribute_hash_payload, status_variant_attribute_payload, status_yaml_payload,
};

pub(crate) use push_commands::push_command;

pub use materialization::projection_to_resource_map;
pub use resource_utils::{
    clean_name, extract_variable_names_from_code, join_under_root, rel_under_root,
    remove_comments_from_code,
};

pub(crate) use materialization::{
    FlowImportPathMaps, PromptReferenceMaps, flow_import_path_maps_from_projection,
    prompt_reference_maps_from_projection, replace_flow_import_names_with_ids,
    replace_resource_names_with_ids,
};

pub const PYTHON_VARIANT_STATUS_KEY_PREFIX: &str = "__python_variant__/";

pub(crate) fn rules_references_from_projection(projection: &Value) -> Option<RulesReferences> {
    let references = projection.pointer("/agentSettings/rules/references")?;
    let refs = RulesReferences {
        sms: json_bool_map(references.get("sms")),
        handoff: json_bool_map(references.get("handoff")),
        attributes: json_bool_map(references.get("attributes")),
        global_functions: json_bool_map(
            references
                .get("globalFunctions")
                .or_else(|| references.get("global_functions")),
        ),
        variables: json_bool_map(references.get("variables")),
        translations: json_bool_map(references.get("translations")),
    };
    if refs.sms.is_empty()
        && refs.handoff.is_empty()
        && refs.attributes.is_empty()
        && refs.global_functions.is_empty()
        && refs.variables.is_empty()
        && refs.translations.is_empty()
    {
        None
    } else {
        Some(refs)
    }
}

pub(crate) fn rules_references_from_behaviour(behaviour: &str) -> Option<RulesReferences> {
    let mut variables = extract_template_references(behaviour, "vrbl");
    variables.extend(extract_template_references(behaviour, "var"));
    let refs = RulesReferences {
        sms: extract_template_references(behaviour, "sms"),
        handoff: extract_template_references(behaviour, "ho"),
        attributes: extract_template_references(behaviour, "attr"),
        global_functions: extract_template_references(behaviour, "fn"),
        variables,
        translations: HashMap::new(),
    };
    if refs.sms.is_empty()
        && refs.handoff.is_empty()
        && refs.attributes.is_empty()
        && refs.global_functions.is_empty()
        && refs.variables.is_empty()
        && refs.translations.is_empty()
    {
        None
    } else {
        Some(refs)
    }
}

fn extract_template_references(behaviour: &str, prefix: &str) -> HashMap<String, bool> {
    let marker = format!("{{{{{prefix}:");
    let mut out = HashMap::new();
    let mut start = 0;
    while let Some(index) = behaviour[start..].find(&marker) {
        let value_start = start + index + marker.len();
        let tail = &behaviour[value_start..];
        let Some(end) = tail.find("}}") else {
            break;
        };
        let value = tail[..end].trim();
        if !value.is_empty() {
            out.insert(value.to_string(), true);
        }
        start = value_start + end + 2;
    }
    out
}

fn json_bool_map(value: Option<&Value>) -> HashMap<String, bool> {
    value
        .and_then(Value::as_object)
        .map(|object| {
            object
                .iter()
                .map(|(key, value)| (key.clone(), value.as_bool().unwrap_or(true)))
                .collect()
        })
        .unwrap_or_default()
}

fn snake_case_json_keys(value: &mut Value) {
    match value {
        Value::Object(object) => {
            let old = std::mem::take(object);
            for (key, mut value) in old {
                snake_case_json_keys(&mut value);
                object.insert(to_snake_case(&key), value);
            }
        }
        Value::Array(items) => {
            for item in items {
                snake_case_json_keys(item);
            }
        }
        _ => {}
    }
}

pub(crate) fn extract_entities_map(root: &Value, path: &[&str]) -> HashMap<String, Value> {
    let mut cur = root;
    for key in path {
        cur = match cur.get(*key) {
            Some(v) => v,
            None => return HashMap::new(),
        };
    }
    cur.as_object()
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default()
}

fn extract_entities_vec(root: &Value, path: &[&str]) -> Vec<(String, Value)> {
    let mut cur = root;
    for key in path {
        cur = match cur.get(*key) {
            Some(v) => v,
            None => return Vec::new(),
        };
    }

    let (entities, ids) = match cur.get("entities").and_then(Value::as_object) {
        Some(entities) => (entities, cur.get("ids").and_then(Value::as_array)),
        None => match cur.as_object() {
            Some(entities) => (entities, None),
            None => return Vec::new(),
        },
    };

    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(ids) = ids {
        for id in ids.iter().filter_map(Value::as_str) {
            if let Some(entity) = entities.get(id) {
                out.push((id.to_string(), entity.clone()));
                seen.insert(id.to_string());
            }
        }
    }
    out.extend(
        entities
            .iter()
            .filter(|(id, _)| !seen.contains(*id))
            .map(|(id, entity)| (id.clone(), entity.clone())),
    );
    out
}

pub(crate) fn to_camel_case(s: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for ch in s.chars() {
        if ch == '_' || ch == '-' {
            upper = true;
            continue;
        }
        if upper {
            out.push(ch.to_ascii_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    out
}

fn to_snake_case(s: &str) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_ascii_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

pub(crate) fn is_synthetic_local_resource_id(resource_id: &str) -> bool {
    let trimmed = resource_id.trim();
    trimmed.is_empty()
        || trimmed == "local"
        || trimmed.contains('/')
        || trimmed.ends_with(".yaml")
        || trimmed.ends_with(".yml")
        || trimmed.ends_with(".py")
}

fn yaml_get<'a>(config: Option<&'a serde_yaml::Value>, key: &str) -> Option<&'a serde_yaml::Value> {
    config.and_then(|c| c.get(key))
}

fn yaml_string(config: Option<&serde_yaml::Value>, key: &str) -> String {
    yaml_get(config, key)
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub(crate) fn yaml_str(config: &serde_yaml::Value, key: &str) -> String {
    yaml_string(Some(config), key)
}

#[cfg(test)]
mod parity_matrix_tests;
#[cfg(test)]
mod tests;
