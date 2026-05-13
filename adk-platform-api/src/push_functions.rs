//! Push commands for global functions and special start/end functions.

use crate::push_single_file_resources::CommandGroups;
use crate::{
    clean_name, extract_entities_map, extract_variable_names_from_code,
    generated_or_stable_resource_id, is_synthetic_local_resource_id, push_command,
};
use adk_domain::ResourceMap;
use adk_protobuf::Metadata;
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::end_function::{
    EndFunctionCreate, EndFunctionDelete, EndFunctionReferences, EndFunctionUpdate,
};
use adk_protobuf::functions::{
    ErrorsUpdate, FunctionCreateFunction, FunctionDeleteFunction, FunctionError,
    FunctionParameterUpdate, FunctionUpdateFunction, ParametersUpdate,
};
use adk_protobuf::start_function::{
    StartFunctionCreate, StartFunctionDelete, StartFunctionReferences, StartFunctionUpdate,
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub(crate) fn function_resource_command_groups(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> CommandGroups {
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
        let name = path
            .split('/')
            .next_back()
            .unwrap_or_default()
            .trim_end_matches(".py")
            .to_string();

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
            let function_code = function_code_from_local_content(content);
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
                (!resource.resource_id.trim().is_empty() && resource.resource_id != "local")
                    .then_some(resource.resource_id.clone())
            })
            .unwrap_or_else(|| format!("function-{}", clean_name(&name).to_lowercase()));
        let function_code = function_code_from_local_content(content);
        let inferred_description = infer_function_description(content);
        let inferred_parameters = infer_function_parameters(&function_code);

        if remote_functions.contains_key(&name) {
            let remote_code = remote_function
                .and_then(|(_, function)| function.get("code").and_then(Value::as_str));
            let remote_description = remote_function
                .and_then(|(_, function)| function.get("description").and_then(Value::as_str));
            let description_changed = !inferred_description.is_empty()
                && remote_description != Some(inferred_description.as_str());
            if remote_code == Some(function_code.as_str()) && !description_changed {
                continue;
            }
            let description = if description_changed {
                Some(inferred_description.clone())
            } else {
                remote_function
                    .and_then(|(_, function)| function.get("description").and_then(Value::as_str))
                    .map(ToString::to_string)
                    .or_else(|| {
                        (!inferred_description.is_empty()).then_some(inferred_description.clone())
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
                    references: None,
                    archived: remote_function
                        .and_then(|(_, function)| function.get("archived").and_then(Value::as_bool))
                        .or(Some(false)),
                }),
            );
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
                    latency_control: None,
                    references: None,
                    archived: Some(false),
                }),
            );
        }
    }

    for (name, (id, _)) in remote_functions {
        if !local_function_names.contains(&name) {
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

pub(crate) fn function_entries(projection: &Value) -> HashMap<String, Value> {
    extract_entities_map(projection, &["functions", "functions", "entities"])
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
    let mut content = String::new();
    if let Some(description) = function.get("description").and_then(Value::as_str)
        && !description.is_empty()
    {
        content.push_str("@func_description(");
        content.push_str(&python_string_literal(description));
        content.push_str(")\n");
    }
    content.push_str(code);
    content
}

fn variable_reference_ids_from_code(code: &str, projection: &Value) -> HashMap<String, bool> {
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

fn python_string_literal(value: &str) -> String {
    let quote = if value.contains('\'') && !value.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::new();
    out.push(quote);
    for ch in value.chars() {
        if ch == '\\' || ch == quote {
            out.push('\\');
        }
        match ch {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push(quote);
    out
}

pub(crate) fn function_code_from_local_content(content: &str) -> String {
    let mut out = String::new();
    for line in content.lines() {
        let trimmed = line.trim_start();
        if line == "from _gen import *  # <AUTO GENERATED>"
            || line == "from imports import *  # <AUTO GENERATED>"
            || trimmed.starts_with("@func_description(")
            || trimmed.starts_with("@func_parameter(")
            || trimmed.starts_with("@func_latency_control(")
        {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if !content.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    out.trim_start_matches('\n').to_string()
}

fn infer_function_description(code: &str) -> String {
    if let Some(description) = function_description_decorator(code) {
        return description;
    }
    let mut in_docstring = false;
    let mut delimiter = "";
    for raw in code.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if !in_docstring && (line.starts_with("\"\"\"") || line.starts_with("'''")) {
            delimiter = if line.starts_with("\"\"\"") {
                "\"\"\""
            } else {
                "'''"
            };
            let stripped = line.trim_start_matches(delimiter).trim();
            if let Some((first, _)) = stripped.split_once(delimiter) {
                return first.trim().to_string();
            }
            if !stripped.is_empty() {
                return stripped.to_string();
            }
            in_docstring = true;
            continue;
        }
        if in_docstring {
            if line.contains(delimiter) {
                return line
                    .split(delimiter)
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_string();
            }
            return line.to_string();
        }
    }
    String::new()
}

fn function_description_decorator(code: &str) -> Option<String> {
    for line in code.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("@func_description(") else {
            continue;
        };
        let arg = rest.strip_suffix(')').unwrap_or(rest).trim();
        return Some(parse_python_string_literal(arg));
    }
    None
}

fn parse_python_string_literal(value: &str) -> String {
    let mut chars = value.chars();
    let Some(quote @ ('\'' | '"')) = chars.next() else {
        return value.to_string();
    };
    let mut out = String::new();
    let mut escaped = false;
    for ch in chars {
        if escaped {
            match ch {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                other => out.push(other),
            }
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == quote {
            break;
        } else {
            out.push(ch);
        }
    }
    out
}

fn infer_function_parameters(code: &str) -> Vec<FunctionParameterUpdate> {
    let signature = code
        .lines()
        .find_map(|line| line.trim().strip_prefix("def ").map(ToString::to_string));
    let Some(signature) = signature else {
        return vec![];
    };
    let Some(open) = signature.find('(') else {
        return vec![];
    };
    let Some(close) = signature[open + 1..].find(')') else {
        return vec![];
    };
    let params = &signature[open + 1..open + 1 + close];
    params
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty() && *p != "self" && *p != "conv")
        .map(|p| p.split('=').next().unwrap_or_default().trim())
        .map(|p| p.split(':').next().unwrap_or_default().trim())
        .filter(|p| !p.is_empty())
        .map(|name| FunctionParameterUpdate {
            id: clean_name(name).to_lowercase(),
            name: name.to_string(),
            description: String::new(),
            r#type: "string".to_string(),
        })
        .collect()
}

fn function_parameters_update_from_projection(function: &Value) -> Option<ParametersUpdate> {
    let parameters = function.get("parameters")?.as_array()?;
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

fn function_errors_update_from_projection(function: &Value) -> Option<ErrorsUpdate> {
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
