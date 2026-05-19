//! Push commands for global functions and special start/end functions.

use crate::push_single_file_resources::CommandGroups;
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
    DelayResponseUpdate, DelayResponsesUpdate, ErrorsUpdate, FunctionCreateFunction,
    FunctionCreateLatencyControl, FunctionDelayResponse, FunctionDeleteFunction, FunctionError,
    FunctionParameterUpdate, FunctionReferences, FunctionUpdateFunction,
    FunctionUpdateLatencyControl, ParametersUpdate,
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
        for parameter in parameters.parameters {
            if parameter.name.is_empty() {
                continue;
            }
            decorators.push(format!(
                "@func_parameter({}, {})\n",
                python_string_literal(&parameter.name),
                python_string_literal(&parameter.description)
            ));
        }
    }
    let latency = latency_control_from_projection(function);
    if latency.enabled {
        let mut parts = vec![
            format!("delay_before_responses_start={}", latency.initial_delay),
            format!("silence_after_each_response={}", latency.interval),
        ];
        if !latency.delay_responses.is_empty() {
            let responses: Vec<_> = latency
                .delay_responses
                .iter()
                .map(|r| format!("({}, {})", python_string_literal(&r.message), r.duration))
                .collect();
            parts.push(format!("delay_responses=[{}]", responses.join(", ")));
        }
        decorators.push(format!("@func_latency_control({})\n", parts.join(", ")));
    }
    insert_python_function_decorators(code, name, decorators)
}

fn insert_python_function_decorators(
    code: &str,
    function_name: &str,
    decorators: Vec<String>,
) -> String {
    if decorators.is_empty() {
        return code.to_string();
    }
    let lines = code.split_inclusive('\n').collect::<Vec<_>>();
    if lines.is_empty() {
        return code.to_string();
    }
    let target_idx = lines.iter().position(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with(&format!("def {function_name}("))
            || trimmed.starts_with(&format!("async def {function_name}("))
    });
    let Some(target_idx) = target_idx else {
        let mut out = decorators.concat();
        out.push_str(code);
        return out;
    };
    let indent = lines[target_idx]
        .chars()
        .take_while(|ch| ch.is_whitespace())
        .collect::<String>();
    let mut insert_at = target_idx;
    while insert_at > 0 && lines[insert_at - 1].trim_start().starts_with('@') {
        insert_at -= 1;
    }
    let decorator_block = decorators
        .into_iter()
        .map(|decorator| format!("{indent}{decorator}"))
        .collect::<String>();
    let mut out = String::new();
    for line in &lines[..insert_at] {
        out.push_str(line);
    }
    out.push_str(&decorator_block);
    for line in &lines[insert_at..] {
        out.push_str(line);
    }
    out
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
    let mut skipping_decorator: Option<DecoratorCallScan> = None;
    for line in content.lines() {
        let trimmed = line.trim_start();
        if let Some(mut state) = skipping_decorator.take() {
            if !state.scan(line.trim()) {
                skipping_decorator = Some(state);
            }
            continue;
        }
        if line == "from _gen import *  # <AUTO GENERATED>"
            || line == "from imports import *  # <AUTO GENERATED>"
        {
            continue;
        }
        if let Some(rest) = adk_decorator_call_rest(trimmed) {
            let mut state = DecoratorCallScan::default();
            if !state.scan(rest) {
                skipping_decorator = Some(state);
            }
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

pub(crate) fn infer_function_description(code: &str) -> String {
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
    adk_decorator_args(code, "func_description")
        .into_iter()
        .filter_map(|args| parse_python_string_args(&args).into_iter().next())
        .next()
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

pub(crate) fn infer_function_parameters(code: &str) -> Vec<FunctionParameterUpdate> {
    let decorator_descriptions = function_parameter_decorators(code);
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
        .filter(|p| !p.is_empty())
        .map(|p| p.split('=').next().unwrap_or_default().trim())
        .filter_map(|p| {
            let (name, annotation) = p
                .split_once(':')
                .map(|(name, annotation)| (name.trim(), Some(annotation.trim())))
                .unwrap_or((p.trim(), None));
            (!name.is_empty() && !matches!(name, "self" | "conv" | "flow"))
                .then_some((name, annotation))
        })
        .map(|(name, annotation)| FunctionParameterUpdate {
            id: generated_or_stable_resource_id("function_parameter", "PARAMETER", name, name),
            name: name.to_string(),
            description: decorator_descriptions
                .get(name)
                .cloned()
                .unwrap_or_default(),
            r#type: annotation
                .and_then(schema_type_from_python_annotation)
                .unwrap_or("string")
                .to_string(),
        })
        .collect()
}

fn function_parameter_decorators(code: &str) -> HashMap<String, String> {
    adk_decorator_args(code, "func_parameter")
        .into_iter()
        .filter_map(|args| {
            let args = parse_python_string_args(&args);
            (args.len() == 2).then(|| (args[0].clone(), args[1].clone()))
        })
        .collect()
}

fn adk_decorator_args(code: &str, decorator_name: &str) -> Vec<String> {
    let prefix = format!("@{decorator_name}(");
    let mut calls = Vec::new();
    let mut active: Option<DecoratorCallScan> = None;
    for raw in code.lines() {
        if let Some(mut state) = active.take() {
            state.args.push('\n');
            if state.scan(raw.trim()) {
                calls.push(state.args.trim().trim_end_matches(',').to_string());
            } else {
                active = Some(state);
            }
            continue;
        }

        let Some(rest) = raw.trim().strip_prefix(&prefix) else {
            continue;
        };
        let mut state = DecoratorCallScan::default();
        if state.scan(rest) {
            calls.push(state.args.trim().trim_end_matches(',').to_string());
        } else {
            active = Some(state);
        }
    }
    calls
}

fn adk_decorator_call_rest(line: &str) -> Option<&str> {
    [
        "@func_description(",
        "@func_parameter(",
        "@func_latency_control(",
    ]
    .iter()
    .find_map(|prefix| line.strip_prefix(prefix))
}

#[derive(Default)]
struct DecoratorCallScan {
    args: String,
    quote: Option<char>,
    escaped: bool,
    depth: i32,
}

impl DecoratorCallScan {
    fn scan(&mut self, fragment: &str) -> bool {
        for ch in fragment.chars() {
            if let Some(quote) = self.quote {
                self.args.push(ch);
                if self.escaped {
                    self.escaped = false;
                } else if ch == '\\' {
                    self.escaped = true;
                } else if ch == quote {
                    self.quote = None;
                }
                continue;
            }

            match ch {
                '\'' | '"' => {
                    self.quote = Some(ch);
                    self.args.push(ch);
                }
                '(' | '[' | '{' => {
                    self.depth += 1;
                    self.args.push(ch);
                }
                ')' => {
                    if self.depth == 0 {
                        return true;
                    }
                    self.depth -= 1;
                    self.args.push(ch);
                }
                ']' | '}' => {
                    self.depth -= 1;
                    self.args.push(ch);
                }
                _ => self.args.push(ch),
            }
        }
        false
    }
}

fn parse_python_string_args(value: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;
    for ch in value.chars() {
        if let Some(active_quote) = quote {
            current.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == active_quote {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => {
                quote = Some(ch);
                current.push(ch);
            }
            ',' => {
                args.push(parse_python_string_literal(current.trim()));
                current.clear();
            }
            other => current.push(other),
        }
    }
    if !current.trim().is_empty() {
        args.push(parse_python_string_literal(current.trim()));
    }
    args
}

fn schema_type_from_python_annotation(annotation: &str) -> Option<&'static str> {
    match annotation
        .split([' ', ')', ','])
        .next()
        .unwrap_or_default()
        .trim()
    {
        "str" => Some("string"),
        "int" => Some("integer"),
        "float" => Some("number"),
        "bool" => Some("boolean"),
        _ => None,
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ParsedLatencyControl {
    pub(crate) enabled: bool,
    pub(crate) initial_delay: i32,
    pub(crate) interval: i32,
    pub(crate) delay_responses: Vec<ParsedDelayResponse>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedDelayResponse {
    pub(crate) id: Option<String>,
    pub(crate) message: String,
    pub(crate) duration: i32,
}

pub(crate) fn local_latency_control_from_code(
    code: &str,
    known_function: Option<&Value>,
) -> ParsedLatencyControl {
    let Some(args) = latency_control_decorator_args(code) else {
        return ParsedLatencyControl::default();
    };
    let known = known_function
        .map(latency_control_from_projection)
        .unwrap_or_default();
    let mut used_ids = HashSet::new();
    let delay_responses = latency_delay_response_args(&args)
        .into_iter()
        .map(|(message, duration)| {
            let id = known
                .delay_responses
                .iter()
                .find(|response| {
                    response.message == message
                        && response
                            .id
                            .as_ref()
                            .is_some_and(|id| !used_ids.contains(id))
                })
                .and_then(|response| response.id.clone())
                .or_else(|| {
                    (!message.trim().is_empty()).then(|| {
                        generated_or_stable_resource_id(
                            "delay_response",
                            "DELAY",
                            &message,
                            &message,
                        )
                    })
                });
            if let Some(id) = &id {
                used_ids.insert(id.clone());
            }
            ParsedDelayResponse {
                id,
                message,
                duration,
            }
        })
        .collect();
    ParsedLatencyControl {
        enabled: true,
        initial_delay: latency_i32_arg(&args, "delay_before_responses_start").unwrap_or(0),
        interval: latency_i32_arg(&args, "silence_after_each_response").unwrap_or(0),
        delay_responses,
    }
}

pub(crate) fn function_create_latency_control(
    latency: &ParsedLatencyControl,
) -> Option<FunctionCreateLatencyControl> {
    latency.enabled.then(|| FunctionCreateLatencyControl {
        enabled: true,
        delay_responses: latency
            .delay_responses
            .iter()
            .map(function_delay_response)
            .collect(),
        initial_delay: latency.initial_delay,
        interval: latency.interval,
    })
}

pub(crate) fn function_update_latency_control(
    function_id: &str,
    latency: &ParsedLatencyControl,
) -> FunctionUpdateLatencyControl {
    FunctionUpdateLatencyControl {
        function_id: function_id.to_string(),
        enabled: latency.enabled,
        delay_responses: Some(DelayResponsesUpdate {
            delay_responses: latency
                .delay_responses
                .iter()
                .map(|response| DelayResponseUpdate {
                    id: response.id.clone().unwrap_or_else(|| {
                        generated_or_stable_resource_id(
                            "delay_response",
                            "DELAY",
                            &response.message,
                            &response.message,
                        )
                    }),
                    message: response.message.clone(),
                    duration: response.duration,
                    references: None,
                })
                .collect(),
        }),
        initial_delay: Some(if latency.enabled {
            latency.initial_delay
        } else {
            0
        }),
        interval: Some(if latency.enabled { latency.interval } else { 0 }),
    }
}

pub(crate) fn latency_control_from_projection(function: &Value) -> ParsedLatencyControl {
    let Some(latency) = function
        .get("latencyControl")
        .or_else(|| function.get("latency_control"))
        .and_then(Value::as_object)
    else {
        return ParsedLatencyControl::default();
    };
    let enabled = latency
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let initial_delay = latency
        .get("initialDelay")
        .or_else(|| latency.get("initial_delay"))
        .and_then(Value::as_i64)
        .unwrap_or(0) as i32;
    let interval = latency.get("interval").and_then(Value::as_i64).unwrap_or(0) as i32;
    let delay_responses = latency
        .get("delayResponses")
        .or_else(|| latency.get("delay_responses"))
        .map(delay_responses_from_projection)
        .unwrap_or_default();
    ParsedLatencyControl {
        enabled,
        initial_delay,
        interval,
        delay_responses,
    }
}

fn function_delay_response(response: &ParsedDelayResponse) -> FunctionDelayResponse {
    FunctionDelayResponse {
        id: response.id.clone().unwrap_or_else(|| {
            generated_or_stable_resource_id(
                "delay_response",
                "DELAY",
                &response.message,
                &response.message,
            )
        }),
        message: response.message.clone(),
        duration: response.duration,
        created_at: None,
        created_by: String::new(),
        updated_at: None,
        updated_by: String::new(),
        references: None,
    }
}

fn latency_control_decorator_args(code: &str) -> Option<String> {
    adk_decorator_args(code, "func_latency_control")
        .into_iter()
        .next()
}

fn latency_i32_arg(args: &str, name: &str) -> Option<i32> {
    let start = args.find(&format!("{name}="))? + name.len() + 1;
    let rest = &args[start..];
    let digits = rest
        .chars()
        .skip_while(|ch| ch.is_whitespace())
        .take_while(|ch| ch.is_ascii_digit() || *ch == '-')
        .collect::<String>();
    digits.parse().ok()
}

fn latency_delay_response_args(args: &str) -> Vec<(String, i32)> {
    let Some(start) = args.find("delay_responses=") else {
        return vec![];
    };
    let mut out = Vec::new();
    let mut chars = args[start..].chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '(' {
            continue;
        }
        while matches!(chars.peek(), Some(ch) if ch.is_whitespace()) {
            chars.next();
        }
        let Some(quote @ ('\'' | '"')) = chars.next() else {
            continue;
        };
        let mut message = String::new();
        let mut escaped = false;
        for ch in chars.by_ref() {
            if escaped {
                message.push(ch);
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote {
                break;
            } else {
                message.push(ch);
            }
        }
        for ch in chars.by_ref() {
            if ch == ',' {
                break;
            }
        }
        let mut duration = String::new();
        while let Some(ch) = chars.peek() {
            if ch.is_ascii_digit() || *ch == '-' {
                duration.push(*ch);
                chars.next();
            } else if ch.is_whitespace() {
                chars.next();
            } else {
                break;
            }
        }
        if let Ok(duration) = duration.parse() {
            out.push((message, duration));
        }
    }
    out
}

fn delay_responses_from_projection(value: &Value) -> Vec<ParsedDelayResponse> {
    if let Some(items) = value.as_array() {
        return items.iter().filter_map(delay_response_from_value).collect();
    }
    let Some(object) = value.as_object() else {
        return vec![];
    };
    if let (Some(entities), Some(ids)) = (
        object.get("entities").and_then(Value::as_object),
        object.get("ids").and_then(Value::as_array),
    ) {
        return ids
            .iter()
            .filter_map(Value::as_str)
            .filter_map(|id| entities.get(id))
            .filter_map(delay_response_from_value)
            .collect();
    }
    vec![]
}

fn delay_response_from_value(value: &Value) -> Option<ParsedDelayResponse> {
    Some(ParsedDelayResponse {
        id: value
            .get("id")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        message: value
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        duration: value.get("duration").and_then(Value::as_i64).unwrap_or(0) as i32,
    })
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
