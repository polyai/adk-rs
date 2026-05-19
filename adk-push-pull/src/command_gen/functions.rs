//! Push commands for global functions and special start/end functions.

use super::single_file_resources::CommandGroups;
pub(crate) use crate::function_parsing::{
    annotated_function_parameter_names, function_code_from_local_content,
    function_create_latency_control, function_update_latency_control, infer_function_description,
    infer_function_parameters, insert_python_function_decorators, latency_control_from_projection,
    local_latency_control_from_code, python_string_literal,
};
use crate::{
    extract_entities_map, extract_variable_names_from_code, flow_import_path_maps_from_projection,
    generated_or_stable_resource_id, is_synthetic_local_resource_id, push_command,
    replace_flow_import_names_with_ids,
};
use adk_protobuf::Metadata;
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::end_function::{
    EndFunctionCreate, EndFunctionDelete, EndFunctionReferences, EndFunctionUpdate,
};
use adk_protobuf::functions::{
    ErrorsUpdate, FunctionCreateFunction, FunctionDeleteFunction, FunctionError,
    FunctionParameterUpdate, FunctionReferences, FunctionUpdateFunction, ParametersUpdate,
};
use adk_protobuf::start_function::{
    StartFunctionCreate, StartFunctionDelete, StartFunctionReferences, StartFunctionUpdate,
};
use adk_types::{Resource, ResourceMap};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// Builds phase-ordered push commands for top-level Python functions.
///
/// This compares the local `functions/*.py` resources with the function state in
/// the Agent Studio projection and emits create/update/delete commands for global
/// functions plus the special start/end functions. It also performs the local
/// Python-to-Agent-Studio translation that is easy to miss at call sites:
/// metadata decorators become descriptions/parameters, latency decorators become
/// separate latency-control commands, variable references are resolved, and
/// flow-function import paths are converted back to projection IDs.
///
/// The returned groups preserve the push phase ordering expected by the Python
/// ADK: deletes first, then creates, updates, and finally post-updates such as
/// latency-control changes.
pub(crate) fn function_resource_command_groups(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> CommandGroups {
    let flow_import_path_maps = flow_import_path_maps_from_projection(projection);
    let remote_functions = function_entries(projection)
        .into_iter()
        .map(|(id, function)| {
            (
                function
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(id.as_str())
                    .to_string(),
                (id, function),
            )
        })
        .collect::<HashMap<_, _>>();
    let remote_start_function = special_function_entry(projection, SpecialFunctionKind::Start);
    let remote_end_function = special_function_entry(projection, SpecialFunctionKind::End);
    let mut local_function_names = HashSet::new();
    let mut has_local_start_function = false;
    let mut has_local_end_function = false;
    let mut groups = CommandGroups::default();

    for resource in resources.values() {
        let path = resource.file_path.as_str();
        if !path.starts_with("functions/") || !path.ends_with(".py") {
            continue;
        }
        let content = resource
            .payload
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let name = local_function_name(path, resource, content);

        if let Some(kind) = special_function_kind_from_path(path) {
            match kind {
                SpecialFunctionKind::Start => has_local_start_function = true,
                SpecialFunctionKind::End => has_local_end_function = true,
            }
            let remote_function = match kind {
                SpecialFunctionKind::Start => remote_start_function.as_ref(),
                SpecialFunctionKind::End => remote_end_function.as_ref(),
            };
            let id = remote_function
                .map(|(id, _)| id.clone())
                .or_else(|| {
                    (!is_synthetic_local_resource_id(&resource.resource_id))
                        .then_some(resource.resource_id.clone())
                })
                .unwrap_or_else(|| {
                    generated_or_stable_resource_id("function", "FUNCTIONS", &name, path)
                });
            let function_code = replace_flow_import_names_with_ids(
                &function_code_from_local_content(content),
                &flow_import_path_maps,
            );
            let inferred_description = infer_function_description(content);
            let variable_references = variable_reference_ids_from_code(&function_code, projection);

            if let Some((_, remote_function)) = remote_function {
                let remote_code = remote_function.get("code").and_then(Value::as_str);
                let remote_description = remote_function.get("description").and_then(Value::as_str);
                let description_changed = !inferred_description.is_empty()
                    && remote_description != Some(inferred_description.as_str());
                if remote_code == Some(function_code.as_str()) && !description_changed {
                    continue;
                }
                let description = if description_changed {
                    Some(inferred_description.clone())
                } else {
                    remote_function
                        .get("description")
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                        .or_else(|| {
                            (!inferred_description.is_empty())
                                .then_some(inferred_description.clone())
                        })
                };
                let errors = function_errors_update_from_projection(remote_function);
                match kind {
                    SpecialFunctionKind::Start => push_command(
                        &mut groups.updates,
                        metadata,
                        "update_start_function",
                        CommandPayload::UpdateStartFunction(StartFunctionUpdate {
                            id: id.clone(),
                            description,
                            code: Some(function_code.clone()),
                            errors,
                            references: Some(StartFunctionReferences {
                                variables: variable_references,
                            }),
                        }),
                    ),
                    SpecialFunctionKind::End => push_command(
                        &mut groups.updates,
                        metadata,
                        "update_end_function",
                        CommandPayload::UpdateEndFunction(EndFunctionUpdate {
                            id: id.clone(),
                            description,
                            code: Some(function_code.clone()),
                            errors,
                            references: Some(EndFunctionReferences {
                                variables: variable_references,
                            }),
                        }),
                    ),
                }
            } else {
                match kind {
                    SpecialFunctionKind::Start => push_command(
                        &mut groups.creates,
                        metadata,
                        "create_start_function",
                        CommandPayload::CreateStartFunction(StartFunctionCreate {
                            id: id.clone(),
                            name: name.clone(),
                            description: inferred_description,
                            parameters: vec![],
                            code: function_code,
                            errors: vec![],
                            archived: Some(false),
                            references: Some(StartFunctionReferences {
                                variables: variable_references,
                            }),
                        }),
                    ),
                    SpecialFunctionKind::End => push_command(
                        &mut groups.creates,
                        metadata,
                        "create_end_function",
                        CommandPayload::CreateEndFunction(EndFunctionCreate {
                            id: id.clone(),
                            name: name.clone(),
                            description: inferred_description,
                            parameters: vec![],
                            code: function_code,
                            errors: vec![],
                            archived: Some(false),
                            references: Some(EndFunctionReferences {
                                variables: variable_references,
                            }),
                        }),
                    ),
                }
            }
            continue;
        }

        local_function_names.insert(name.clone());
        let remote_function = remote_functions.get(&name);
        let id = remote_functions
            .get(&name)
            .map(|(id, _)| id.clone())
            .or_else(|| {
                (!is_synthetic_local_resource_id(&resource.resource_id))
                    .then_some(resource.resource_id.clone())
            })
            .unwrap_or_else(|| {
                generated_or_stable_resource_id("function", "FUNCTIONS", &name, path)
            });
        let function_code = replace_flow_import_names_with_ids(
            &function_code_from_local_content(content),
            &flow_import_path_maps,
        );
        let inferred_description = infer_function_description(content);
        let inferred_parameters = infer_function_parameters(content);
        let variable_references = variable_reference_ids_from_code(&function_code, projection);
        let local_latency =
            local_latency_control_from_code(content, remote_function.map(|(_, function)| function));

        if remote_functions.contains_key(&name) {
            let remote_code = remote_function
                .and_then(|(_, function)| function.get("code").and_then(Value::as_str));
            let remote_description = remote_function
                .and_then(|(_, function)| function.get("description").and_then(Value::as_str));
            let description_changed = !inferred_description.is_empty()
                && remote_description != Some(inferred_description.as_str());
            if remote_code != Some(function_code.as_str()) || description_changed {
                let description = if description_changed {
                    Some(inferred_description.clone())
                } else {
                    remote_function
                        .and_then(|(_, function)| {
                            function.get("description").and_then(Value::as_str)
                        })
                        .map(ToString::to_string)
                        .or_else(|| {
                            (!inferred_description.is_empty())
                                .then_some(inferred_description.clone())
                        })
                };
                let parameters = remote_function
                    .and_then(|(_, function)| function_parameters_update_from_projection(function))
                    .or_else(|| {
                        (!inferred_parameters.is_empty()).then_some(ParametersUpdate {
                            parameters: inferred_parameters.clone(),
                        })
                    });
                let errors = remote_function
                    .and_then(|(_, function)| function_errors_update_from_projection(function));
                push_command(
                    &mut groups.updates,
                    metadata,
                    "update_function",
                    CommandPayload::UpdateFunction(FunctionUpdateFunction {
                        id: id.clone(),
                        name: Some(name.clone()),
                        description,
                        parameters,
                        code: Some(function_code.clone()),
                        errors,
                        references: Some(function_references(variable_references)),
                        archived: remote_function
                            .and_then(|(_, function)| {
                                function.get("archived").and_then(Value::as_bool)
                            })
                            .or(Some(false)),
                    }),
                );
            }
            if let Some((_, remote_function)) = remote_function {
                let remote_latency = latency_control_from_projection(remote_function);
                if local_latency != remote_latency {
                    push_command(
                        &mut groups.post_updates,
                        metadata,
                        "update_latency_control",
                        CommandPayload::UpdateLatencyControl(function_update_latency_control(
                            &id,
                            &local_latency,
                        )),
                    );
                }
            }
        } else {
            push_command(
                &mut groups.creates,
                metadata,
                "create_function",
                CommandPayload::CreateFunction(FunctionCreateFunction {
                    id: id.clone(),
                    name: name.clone(),
                    description: inferred_description,
                    parameters: inferred_parameters,
                    code: function_code,
                    errors: vec![],
                    latency_control: function_create_latency_control(&local_latency),
                    references: Some(function_references(variable_references)),
                    archived: Some(false),
                }),
            );
        }
    }

    for (name, (id, function)) in remote_functions {
        if !function_archived(&function) && !local_function_names.contains(&name) {
            push_command(
                &mut groups.deletes,
                metadata,
                "delete_function",
                CommandPayload::DeleteFunction(FunctionDeleteFunction { id }),
            );
        }
    }
    if let Some((id, _)) = remote_start_function
        && !has_local_start_function
    {
        push_command(
            &mut groups.deletes,
            metadata,
            "delete_start_function",
            CommandPayload::DeleteStartFunction(StartFunctionDelete { id }),
        );
    }
    if let Some((id, _)) = remote_end_function
        && !has_local_end_function
    {
        push_command(
            &mut groups.deletes,
            metadata,
            "delete_end_function",
            CommandPayload::DeleteEndFunction(EndFunctionDelete { id }),
        );
    }

    groups
}

fn function_references(variables: HashMap<String, bool>) -> FunctionReferences {
    FunctionReferences {
        flow_steps: HashMap::new(),
        topics: HashMap::new(),
        stop_keywords: HashMap::new(),
        behaviour: HashMap::new(),
        variables,
    }
}

pub(crate) fn function_entries(projection: &Value) -> HashMap<String, Value> {
    extract_entities_map(projection, &["functions", "functions", "entities"])
}

fn function_archived(function: &Value) -> bool {
    function
        .get("archived")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

pub(crate) fn local_function_name(path: &str, resource: &Resource, content: &str) -> String {
    meaningful_resource_name(resource, path)
        .or_else(|| inferred_function_name(content))
        .unwrap_or_else(|| function_file_stem(path).to_string())
}

fn inferred_function_name(content: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let trimmed = line.trim_start();
        let signature = trimmed
            .strip_prefix("def ")
            .or_else(|| trimmed.strip_prefix("async def "))?;
        let open = signature.find('(')?;
        let name = signature[..open].trim();
        (!name.is_empty()).then(|| name.to_string())
    })
}

fn meaningful_resource_name(resource: &Resource, path: &str) -> Option<String> {
    let name = resource.name.trim();
    (!name.is_empty()
        && name != path
        && !name.contains('/')
        && !name.ends_with(".py")
        && !name.ends_with(".yaml")
        && !name.ends_with(".yml"))
    .then(|| name.to_string())
}

fn function_file_stem(path: &str) -> &str {
    path.split('/')
        .next_back()
        .unwrap_or_default()
        .trim_end_matches(".py")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpecialFunctionKind {
    Start,
    End,
}

fn special_function_kind_from_path(path: &str) -> Option<SpecialFunctionKind> {
    match path {
        "functions/start_function.py" => Some(SpecialFunctionKind::Start),
        "functions/end_function.py" => Some(SpecialFunctionKind::End),
        _ => None,
    }
}

fn special_function_projection_key(kind: SpecialFunctionKind) -> &'static str {
    match kind {
        SpecialFunctionKind::Start => "startFunction",
        SpecialFunctionKind::End => "endFunction",
    }
}

pub(crate) fn special_function_name(kind: SpecialFunctionKind) -> &'static str {
    match kind {
        SpecialFunctionKind::Start => "start_function",
        SpecialFunctionKind::End => "end_function",
    }
}

pub(crate) fn special_function_entry(
    projection: &Value,
    kind: SpecialFunctionKind,
) -> Option<(String, Value)> {
    let function = projection
        .get("specialFunctions")?
        .get(special_function_projection_key(kind))?;
    if function
        .get("archived")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return None;
    }
    let id = function
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_else(|| special_function_name(kind))
        .to_string();
    Some((id, function.clone()))
}

pub(crate) fn function_raw_content(function: &Value) -> String {
    let code = function
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let name = function
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut decorators = Vec::new();
    if let Some(description) = function.get("description").and_then(Value::as_str)
        && !description.is_empty()
    {
        decorators.push(format!(
            "@func_description({})\n",
            python_string_literal(description)
        ));
    }
    if let Some(parameters) = function_parameters_update_from_projection(function) {
        let annotated_parameter_names = annotated_function_parameter_names(code);
        for parameter in parameters.parameters {
            if parameter.name.is_empty() || !annotated_parameter_names.contains(&parameter.name) {
                continue;
            }
            decorators.push(format!(
                "@func_parameter({}, {})\n",
                python_string_literal(&parameter.name),
                python_string_literal(&parameter.description)
            ));
        }
    }
    insert_python_function_decorators(code, name, decorators)
}

pub(crate) fn variable_reference_ids_from_code(
    code: &str,
    projection: &Value,
) -> HashMap<String, bool> {
    extract_variable_names_from_code(code)
        .into_iter()
        .map(|name| {
            let id = variable_entries(projection)
                .into_iter()
                .find_map(|(id, variable)| {
                    (variable.get("name").and_then(Value::as_str) == Some(name.as_str()))
                        .then_some(id)
                })
                .unwrap_or_else(|| {
                    generated_or_stable_resource_id(
                        "variable",
                        "VARIABLES",
                        &name,
                        &format!("variables/{name}"),
                    )
                });
            (id, true)
        })
        .collect()
}

fn variable_entries(projection: &Value) -> HashMap<String, Value> {
    extract_entities_map(projection, &["variables", "variables", "entities"])
}

pub(crate) fn function_parameters_update_from_projection(
    function: &Value,
) -> Option<ParametersUpdate> {
    let parameters = function
        .get("parameters")
        .and_then(Value::as_array)
        .cloned()
        .or_else(|| {
            function
                .get("parameters")
                .and_then(|parameters| parameters.get("entities"))
                .and_then(Value::as_object)
                .map(|entities| entities.values().cloned().collect())
        })?;
    let updates: Vec<FunctionParameterUpdate> = parameters
        .iter()
        .map(|p| FunctionParameterUpdate {
            id: p
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            name: p
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            description: p
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            r#type: p
                .get("type")
                .or_else(|| p.get("parameterType"))
                .and_then(Value::as_str)
                .unwrap_or("string")
                .to_string(),
        })
        .collect();
    (!updates.is_empty()).then_some(ParametersUpdate {
        parameters: updates,
    })
}

pub(crate) fn function_errors_update_from_projection(function: &Value) -> Option<ErrorsUpdate> {
    let errors = function.get("errors")?.as_array()?;
    let updates: Vec<FunctionError> = errors
        .iter()
        .map(|e| FunctionError {
            lineno: e.get("lineno").and_then(Value::as_i64).unwrap_or_default() as i32,
            message: e
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            text: e
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        })
        .collect();
    (!updates.is_empty()).then_some(ErrorsUpdate { errors: updates })
}
