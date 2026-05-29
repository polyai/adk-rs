//! Push commands for virtual variable resources derived from `conv.state.*` usage.

use crate::functions;
use crate::ids::stable_resource_id;
use crate::push_commands::CommandGroups;
use crate::{extract_entities_map, extract_variable_names_from_code, push_command};
use adk_protobuf::Metadata;
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::variables::{VariableCreate, VariableDelete, VariableReferences, VariableUpdate};
use adk_types::{Resource, ResourceMap};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Default, Clone)]
struct VariableReferenceTargets {
    functions: HashMap<String, bool>,
    start_functions: HashMap<String, bool>,
    end_functions: HashMap<String, bool>,
}

impl VariableReferenceTargets {
    fn is_empty(&self) -> bool {
        self.functions.is_empty()
            && self.start_functions.is_empty()
            && self.end_functions.is_empty()
    }

    fn to_proto(&self) -> Option<VariableReferences> {
        (!self.is_empty()).then(|| VariableReferences {
            functions: self.functions.clone(),
            delay_responses: HashMap::new(),
            flow_steps: HashMap::new(),
            flow_no_code_steps: HashMap::new(),
            flow_functions: HashMap::new(),
            topics: HashMap::new(),
            behaviours: HashMap::new(),
            greetings: HashMap::new(),
            roles: HashMap::new(),
            personalities: HashMap::new(),
            sms: HashMap::new(),
            start_functions: self.start_functions.clone(),
            end_functions: self.end_functions.clone(),
        })
    }
}

#[derive(Debug, Clone, Copy)]
enum FunctionReferenceKind {
    Global,
    Start,
    End,
}

pub(crate) fn variable_resource_command_groups(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> CommandGroups {
    let remote_variables = remote_variables(projection);
    let variable_references = variable_reference_targets(resources, projection);
    let mut local_variable_names = HashSet::new();
    let mut groups = CommandGroups::default();

    for resource in resources.values() {
        let path = resource.file_path.as_str();
        if !path.starts_with("variables/") {
            continue;
        }
        let name = path.trim_start_matches("variables/").to_string();
        if name.is_empty() {
            continue;
        }
        local_variable_names.insert(name.clone());
        let id = remote_variables
            .get(&name)
            .cloned()
            .or_else(|| local_resource_id(resource))
            .unwrap_or_else(|| stable_resource_id("VARIABLES", &name, path));
        let references = variable_references
            .get(&name)
            .and_then(VariableReferenceTargets::to_proto);
        if remote_variables.contains_key(&name) {
            if references
                .as_ref()
                .is_some_and(|refs| variable_references_match(projection, &name, refs))
            {
                continue;
            }
            push_command(
                &mut groups.updates,
                metadata,
                "variable_update",
                CommandPayload::VariableUpdate(VariableUpdate {
                    id: id.clone(),
                    name: name.clone(),
                    references,
                }),
            );
        } else {
            push_command(
                &mut groups.creates,
                metadata,
                "variable_create",
                CommandPayload::VariableCreate(VariableCreate {
                    id: id.clone(),
                    name: name.clone(),
                    references: None,
                }),
            );
            if references.is_some() {
                push_command(
                    &mut groups.updates,
                    metadata,
                    "variable_update",
                    CommandPayload::VariableUpdate(VariableUpdate {
                        id: id.clone(),
                        name: name.clone(),
                        references,
                    }),
                );
            }
        }
    }

    for (name, id) in &remote_variables {
        if !local_variable_names.contains(name) {
            push_command(
                &mut groups.deletes,
                metadata,
                "variable_delete",
                CommandPayload::VariableDelete(VariableDelete { id: id.clone() }),
            );
        }
    }

    groups
}

fn remote_variables(projection: &Value) -> HashMap<String, String> {
    let entities = extract_entities_map(projection, &["variables", "variables", "entities"]);
    let mut variables = HashMap::new();
    for (id, value) in entities {
        let name = value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&id)
            .to_string();
        variables.insert(name, id);
    }
    variables
}

fn variable_reference_targets(
    resources: &ResourceMap,
    projection: &Value,
) -> HashMap<String, VariableReferenceTargets> {
    let mut targets: HashMap<String, VariableReferenceTargets> = HashMap::new();
    for resource in resources.values() {
        let path = resource.file_path.as_str();
        let content = resource
            .payload
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let Some((kind, function_id)) = function_reference_target(path, resource, projection)
        else {
            continue;
        };
        for name in extract_variable_names_from_code(content) {
            let entry = targets.entry(name).or_default();
            match kind {
                FunctionReferenceKind::Global => {
                    entry.functions.insert(function_id.clone(), true);
                }
                FunctionReferenceKind::Start => {
                    entry.start_functions.insert(function_id.clone(), true);
                }
                FunctionReferenceKind::End => {
                    entry.end_functions.insert(function_id.clone(), true);
                }
            }
        }
    }
    targets
}

fn function_reference_target(
    path: &str,
    resource: &Resource,
    projection: &Value,
) -> Option<(FunctionReferenceKind, String)> {
    if path == "functions/start_function.py" {
        return Some((
            FunctionReferenceKind::Start,
            projection
                .pointer("/specialFunctions/startFunction/id")
                .and_then(Value::as_str)
                .map(ToString::to_string)
                .or_else(|| local_resource_id(resource))
                .unwrap_or_else(|| stable_resource_id("FUNCTIONS", "start_function", path)),
        ));
    }
    if path == "functions/end_function.py" {
        return Some((
            FunctionReferenceKind::End,
            projection
                .pointer("/specialFunctions/endFunction/id")
                .and_then(Value::as_str)
                .map(ToString::to_string)
                .or_else(|| local_resource_id(resource))
                .unwrap_or_else(|| stable_resource_id("FUNCTIONS", "end_function", path)),
        ));
    }
    if path.starts_with("functions/") && path.ends_with(".py") {
        let content = resource
            .payload
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let name = functions::local_function_name(path, resource, content);
        let remote_id = crate::functions::function_entries(projection)
            .into_iter()
            .find_map(|(id, function)| {
                (function.get("name").and_then(Value::as_str) == Some(name.as_str())).then_some(id)
            });
        return Some((
            FunctionReferenceKind::Global,
            remote_id
                .or_else(|| local_resource_id(resource))
                .unwrap_or_else(|| stable_resource_id("FUNCTIONS", &name, path)),
        ));
    }
    None
}

fn local_resource_id(resource: &Resource) -> Option<String> {
    let id = resource.resource_id.trim();
    (!id.is_empty()
        && id != "local"
        && !id.contains('/')
        && !id.ends_with(".py")
        && !id.ends_with(".yaml")
        && !id.ends_with(".yml"))
    .then(|| id.to_string())
}

fn variable_references_match(
    projection: &Value,
    name: &str,
    references: &VariableReferences,
) -> bool {
    let Some(remote) = extract_entities_map(projection, &["variables", "variables", "entities"])
        .into_iter()
        .find_map(|(_, variable)| {
            (variable.get("name").and_then(Value::as_str) == Some(name)).then_some(variable)
        })
    else {
        return false;
    };
    let Some(remote_refs) = remote.get("references") else {
        return references.functions.is_empty()
            && references.start_functions.is_empty()
            && references.end_functions.is_empty();
    };
    bool_map_from_json(remote_refs, "functions", "functions") == references.functions
        && bool_map_from_json(remote_refs, "startFunctions", "start_functions")
            == references.start_functions
        && bool_map_from_json(remote_refs, "endFunctions", "end_functions")
            == references.end_functions
}

fn bool_map_from_json(value: &Value, camel_key: &str, snake_key: &str) -> HashMap<String, bool> {
    value
        .get(camel_key)
        .or_else(|| value.get(snake_key))
        .and_then(Value::as_object)
        .map(|object| {
            object
                .iter()
                .map(|(key, value)| (key.clone(), value.as_bool().unwrap_or(true)))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "command_gen_tests.rs"]
mod command_gen_tests;
