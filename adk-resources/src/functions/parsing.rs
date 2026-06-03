use super::python::normalize_python_module_docstring_import_spacing;
use crate::CommandGenError;
use crate::ids::stable_resource_id;
use adk_protobuf::functions::{
    DelayResponseUpdate, DelayResponsesUpdate, FunctionCreateLatencyControl, FunctionDelayResponse,
    FunctionParameterUpdate, FunctionUpdateLatencyControl,
};
use ruff_python_ast::{
    Arguments, Decorator, ExceptHandler, Expr, ModModule, Number, Stmt, StmtFunctionDef, UnaryOp,
};
use ruff_text_size::{Ranged, TextRange};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::ops::Range;

pub(crate) fn annotated_function_parameter_names(
    code: &str,
    function_name: &str,
) -> HashSet<String> {
    let Some(module) = parse_python_module(code) else {
        return HashSet::new();
    };
    let Some(function) = find_function(&module, function_name) else {
        return HashSet::new();
    };

    function
        .parameters
        .iter_non_variadic_params()
        .filter_map(|parameter| {
            let name = parameter.name().as_str();
            (parameter.annotation().is_some() && should_materialize_parameter(name))
                .then(|| name.to_string())
        })
        .collect()
}

pub(crate) fn python_signature_for_function(code: &str, function_name: &str) -> Option<String> {
    let module = parse_python_module(code)?;
    let function = find_function(&module, function_name)?;
    let name_start = function.name.range.start().to_usize();
    let header_end = function
        .body
        .first()
        .map(|stmt| stmt.start().to_usize())
        .unwrap_or_else(|| function.range.end().to_usize());
    let header = code.get(name_start..header_end)?.trim();
    let header = header.strip_suffix(':').unwrap_or(header).trim_end();
    Some(header.to_string())
}

pub(crate) fn insert_python_function_decorators(
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
        let Some(signature) = trimmed
            .strip_prefix("def ")
            .or_else(|| trimmed.strip_prefix("async def "))
        else {
            return false;
        };
        let Some(open) = signature.find('(') else {
            return false;
        };
        signature[..open].trim() == function_name
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

pub(crate) fn python_string_literal(value: &str) -> String {
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

#[cfg(test)]
pub(crate) fn function_code_from_local_content(content: &str) -> String {
    try_function_code_from_local_content("<local function>", content)
        .expect("valid Python function content")
}

pub(crate) fn try_function_code_from_local_content(
    path: &str,
    content: &str,
) -> Result<String, CommandGenError> {
    let mut ranges = generated_import_line_ranges(content);
    let module = parse_python_module_for_resource(path, content)?;
    collect_adk_decorator_line_ranges(&module.body, content, &mut ranges);
    let raw = remove_source_ranges(content, ranges)
        .trim_start_matches('\n')
        .to_string();
    Ok(normalize_python_module_docstring_import_spacing(&raw))
}

pub(crate) fn infer_function_description(code: &str) -> String {
    if let Some(description) = function_description_decorator(code) {
        return description;
    }
    parse_python_module(code)
        .and_then(|module| first_function(&module).and_then(function_docstring))
        .unwrap_or_default()
}

pub(crate) fn infer_function_parameters(
    code: &str,
    function_name: &str,
) -> Vec<FunctionParameterUpdate> {
    let decorator_descriptions = function_parameter_decorators(code);
    let Some(module) = parse_python_module(code) else {
        return vec![];
    };
    let Some(function) = find_function(&module, function_name) else {
        return vec![];
    };

    function
        .parameters
        .iter_non_variadic_params()
        .filter(|parameter| should_materialize_parameter(parameter.name().as_str()))
        .map(|parameter| {
            let name = parameter.name().as_str();
            let parameter_path = format!("{function_name}.{name}");
            FunctionParameterUpdate {
                id: stable_resource_id("PARAMETER", name, &parameter_path),
                name: name.to_string(),
                description: decorator_descriptions
                    .get(name)
                    .cloned()
                    .unwrap_or_default(),
                r#type: parameter
                    .annotation()
                    .and_then(|annotation| {
                        schema_type_from_python_annotation(expr_source(code, annotation))
                    })
                    .unwrap_or("string")
                    .to_string(),
            }
        })
        .collect()
}

fn function_parameter_decorators(code: &str) -> HashMap<String, String> {
    let Some(module) = parse_python_module(code) else {
        return HashMap::new();
    };
    let mut calls = Vec::new();
    collect_decorator_calls(&module.body, "func_parameter", &mut calls);
    calls
        .into_iter()
        .filter_map(|args| {
            Some((
                string_argument(args, "name", 0)?,
                string_argument(args, "description", 1)?,
            ))
        })
        .collect()
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

fn function_description_decorator(code: &str) -> Option<String> {
    let module = parse_python_module(code)?;
    let mut calls = Vec::new();
    collect_decorator_calls(&module.body, "func_description", &mut calls);
    calls
        .into_iter()
        .find_map(|args| string_argument(args, "description", 0))
}

fn parse_python_module(code: &str) -> Option<ModModule> {
    ruff_python_parser::parse_module(code)
        .ok()
        .map(|parsed| parsed.into_syntax())
}

fn parse_python_module_for_resource(path: &str, code: &str) -> Result<ModModule, CommandGenError> {
    ruff_python_parser::parse_module(code)
        .map(|parsed| parsed.into_syntax())
        .map_err(|error| CommandGenError::PythonSyntax {
            path: path.to_string(),
            message: error.to_string(),
        })
}

fn first_function(module: &ModModule) -> Option<&StmtFunctionDef> {
    first_function_in_statements(&module.body)
}

fn first_function_in_statements(statements: &[Stmt]) -> Option<&StmtFunctionDef> {
    for statement in statements {
        if let Some(function) = sync_or_async_function(statement) {
            return Some(function);
        }
        for body in child_statement_bodies(statement) {
            if let Some(function) = first_function_in_statements(body) {
                return Some(function);
            }
        }
    }
    None
}

fn find_function<'a>(module: &'a ModModule, function_name: &str) -> Option<&'a StmtFunctionDef> {
    find_function_in_statements(&module.body, function_name)
}

fn find_function_in_statements<'a>(
    statements: &'a [Stmt],
    function_name: &str,
) -> Option<&'a StmtFunctionDef> {
    for statement in statements {
        if let Some(function) = sync_or_async_function(statement)
            && function.name.as_str() == function_name
        {
            return Some(function);
        }
        for body in child_statement_bodies(statement) {
            if let Some(function) = find_function_in_statements(body, function_name) {
                return Some(function);
            }
        }
    }
    None
}

/// Return a Ruff function node for both Python `def` and `async def`.
///
/// The Ruff AST version used here collapses Python's separate `FunctionDef` and
/// `AsyncFunctionDef` nodes into `Stmt::FunctionDef`; asyncness is tracked on
/// `StmtFunctionDef::is_async`.
fn sync_or_async_function(statement: &Stmt) -> Option<&StmtFunctionDef> {
    match statement {
        Stmt::FunctionDef(function) => Some(function),
        _ => None,
    }
}

fn function_docstring(function: &StmtFunctionDef) -> Option<String> {
    let Stmt::Expr(statement) = function.body.first()? else {
        return None;
    };
    let Expr::StringLiteral(docstring) = statement.value.as_ref() else {
        return None;
    };
    docstring
        .value
        .to_str()
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToString::to_string)
}

fn should_materialize_parameter(name: &str) -> bool {
    !name.is_empty() && !matches!(name, "self" | "conv" | "flow")
}

fn collect_decorator_calls<'a>(
    statements: &'a [Stmt],
    decorator_name: &str,
    calls: &mut Vec<&'a Arguments>,
) {
    for statement in statements {
        if let Some(function) = sync_or_async_function(statement) {
            calls.extend(
                function
                    .decorator_list
                    .iter()
                    .filter_map(|decorator| decorator_call_arguments(decorator, decorator_name)),
            );
        }
        for body in child_statement_bodies(statement) {
            collect_decorator_calls(body, decorator_name, calls);
        }
    }
}

fn decorator_call_arguments<'a>(
    decorator: &'a Decorator,
    decorator_name: &str,
) -> Option<&'a Arguments> {
    let Expr::Call(call) = &decorator.expression else {
        return None;
    };
    expr_name_matches(&call.func, decorator_name).then_some(&call.arguments)
}

fn expr_name_matches(expr: &Expr, name: &str) -> bool {
    match expr {
        Expr::Name(expr) => expr.id.as_str() == name,
        _ => false,
    }
}

fn string_argument(args: &Arguments, name: &str, position: usize) -> Option<String> {
    args.find_argument_value(name, position)
        .and_then(string_expr_value)
}

fn string_expr_value(expr: &Expr) -> Option<String> {
    match expr {
        Expr::StringLiteral(value) => Some(value.value.to_str().to_string()),
        _ => None,
    }
}

fn expr_source<'a>(code: &'a str, expr: &Expr) -> &'a str {
    source_for_range(code, expr.range()).unwrap_or_default()
}

fn source_for_range(code: &str, range: TextRange) -> Option<&str> {
    code.get(range.start().to_usize()..range.end().to_usize())
}

fn generated_import_line_ranges(content: &str) -> Vec<Range<usize>> {
    let mut offset = 0;
    let mut ranges = Vec::new();
    for line in content.split_inclusive('\n') {
        let line_text = line.trim_end_matches('\n').trim_end_matches('\r');
        if line_text == "from _gen import *  # <AUTO GENERATED>"
            || line_text == "from imports import *  # <AUTO GENERATED>"
        {
            ranges.push(offset..offset + line.len());
        }
        offset += line.len();
    }
    ranges
}

fn collect_adk_decorator_line_ranges(
    statements: &[Stmt],
    content: &str,
    ranges: &mut Vec<Range<usize>>,
) {
    for statement in statements {
        if let Some(function) = sync_or_async_function(statement) {
            ranges.extend(
                function
                    .decorator_list
                    .iter()
                    .filter(|decorator| is_adk_decorator(decorator))
                    .map(|decorator| line_range_for_text_range(content, decorator.range())),
            );
        }
        for body in child_statement_bodies(statement) {
            collect_adk_decorator_line_ranges(body, content, ranges);
        }
    }
}

fn child_statement_bodies(statement: &Stmt) -> Vec<&[Stmt]> {
    match statement {
        Stmt::FunctionDef(function) => vec![function.body.as_slice()],
        Stmt::ClassDef(class_def) => vec![class_def.body.as_slice()],
        Stmt::For(for_loop) => vec![for_loop.body.as_slice(), for_loop.orelse.as_slice()],
        Stmt::While(while_loop) => vec![while_loop.body.as_slice(), while_loop.orelse.as_slice()],
        Stmt::If(if_statement) => {
            let mut bodies = vec![if_statement.body.as_slice()];
            bodies.extend(
                if_statement
                    .elif_else_clauses
                    .iter()
                    .map(|clause| clause.body.as_slice()),
            );
            bodies
        }
        Stmt::With(with_statement) => vec![with_statement.body.as_slice()],
        Stmt::Match(match_statement) => match_statement
            .cases
            .iter()
            .map(|case| case.body.as_slice())
            .collect(),
        Stmt::Try(try_statement) => {
            let mut bodies = vec![try_statement.body.as_slice()];
            bodies.extend(try_statement.handlers.iter().map(except_handler_body));
            bodies.push(try_statement.orelse.as_slice());
            bodies.push(try_statement.finalbody.as_slice());
            bodies
        }
        _ => Vec::new(),
    }
}

fn except_handler_body(handler: &ExceptHandler) -> &[Stmt] {
    match handler {
        ExceptHandler::ExceptHandler(handler) => handler.body.as_slice(),
    }
}

fn is_adk_decorator(decorator: &Decorator) -> bool {
    ["func_description", "func_parameter", "func_latency_control"]
        .iter()
        .any(|name| decorator_call_arguments(decorator, name).is_some())
}

fn line_range_for_text_range(content: &str, range: TextRange) -> Range<usize> {
    let start = range.start().to_usize();
    let end = range.end().to_usize();
    let line_start = content[..start].rfind('\n').map_or(0, |index| index + 1);
    let line_end = content[end..]
        .find('\n')
        .map_or(content.len(), |index| end + index + 1);
    line_start..line_end
}

fn remove_source_ranges(content: &str, mut ranges: Vec<Range<usize>>) -> String {
    ranges.sort_by_key(|range| range.start);
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        if let Some(last) = merged.last_mut()
            && range.start <= last.end
        {
            last.end = last.end.max(range.end);
            continue;
        }
        merged.push(range);
    }

    let mut out = String::new();
    let mut cursor = 0;
    for range in merged {
        if range.start > cursor {
            out.push_str(&content[cursor..range.start]);
        }
        cursor = cursor.max(range.end);
    }
    if cursor < content.len() {
        out.push_str(&content[cursor..]);
    }
    out
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ParsedLatencyDecorator {
    initial_delay: Option<i32>,
    interval: Option<i32>,
    delay_responses: Vec<(String, i32)>,
}

pub(crate) fn local_latency_control_from_code(
    code: &str,
    known_function: Option<&Value>,
) -> ParsedLatencyControl {
    let Some(args) = latency_control_decorator_from_code(code) else {
        return ParsedLatencyControl::default();
    };
    let known = known_function
        .map(latency_control_from_projection)
        .unwrap_or_default();
    let mut used_ids = HashSet::new();
    let delay_responses = args
        .delay_responses
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
                    (!message.trim().is_empty())
                        .then(|| stable_resource_id("DELAY", &message, &message))
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
        initial_delay: args.initial_delay.unwrap_or(0),
        interval: args.interval.unwrap_or(0),
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
                        stable_resource_id("DELAY", &response.message, &response.message)
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
        id: response
            .id
            .clone()
            .unwrap_or_else(|| stable_resource_id("DELAY", &response.message, &response.message)),
        message: response.message.clone(),
        duration: response.duration,
        created_at: None,
        created_by: String::new(),
        updated_at: None,
        updated_by: String::new(),
        references: None,
    }
}

fn latency_control_decorator_from_code(code: &str) -> Option<ParsedLatencyDecorator> {
    let module = parse_python_module(code)?;
    let mut calls = Vec::new();
    collect_decorator_calls(&module.body, "func_latency_control", &mut calls);
    calls.into_iter().next().map(latency_decorator_from_args)
}

fn latency_decorator_from_args(args: &Arguments) -> ParsedLatencyDecorator {
    ParsedLatencyDecorator {
        initial_delay: args
            .find_argument_value("delay_before_responses_start", 0)
            .and_then(int_expr_value),
        interval: args
            .find_argument_value("silence_after_each_response", 1)
            .and_then(int_expr_value),
        delay_responses: args
            .find_argument_value("delay_responses", 2)
            .map(delay_response_args_from_expr)
            .unwrap_or_default(),
    }
}

fn int_expr_value(expr: &Expr) -> Option<i32> {
    match expr {
        Expr::NumberLiteral(number) => match &number.value {
            Number::Int(value) => value.as_i32(),
            _ => None,
        },
        Expr::UnaryOp(unary) => match unary.op {
            UnaryOp::UAdd => int_expr_value(&unary.operand),
            UnaryOp::USub => int_expr_value(&unary.operand).and_then(i32::checked_neg),
            _ => None,
        },
        _ => None,
    }
}

fn delay_response_args_from_expr(expr: &Expr) -> Vec<(String, i32)> {
    expr_sequence_items(expr)
        .map(|items| {
            items
                .iter()
                .filter_map(delay_response_arg_from_expr)
                .collect()
        })
        .unwrap_or_default()
}

fn delay_response_arg_from_expr(expr: &Expr) -> Option<(String, i32)> {
    let items = expr_sequence_items(expr)?;
    let [message, duration] = items else {
        return None;
    };
    Some((string_expr_value(message)?, int_expr_value(duration)?))
}

fn expr_sequence_items(expr: &Expr) -> Option<&[Expr]> {
    match expr {
        Expr::List(list) => Some(&list.elts),
        Expr::Tuple(tuple) => Some(&tuple.elts),
        _ => None,
    }
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
