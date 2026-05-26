use crate::ids::stable_resource_id;
use adk_protobuf::functions::{
    DelayResponseUpdate, DelayResponsesUpdate, FunctionCreateLatencyControl, FunctionDelayResponse,
    FunctionParameterUpdate, FunctionUpdateLatencyControl,
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub(crate) fn annotated_function_parameter_names(
    code: &str,
    function_name: &str,
) -> HashSet<String> {
    let signature = python_signature_for_function(code, function_name);
    let Some(signature) = signature else {
        return HashSet::new();
    };
    let Some(open) = signature.find('(') else {
        return HashSet::new();
    };
    let Some(close) = signature[open + 1..].find(')') else {
        return HashSet::new();
    };
    signature[open + 1..open + 1 + close]
        .split(',')
        .map(str::trim)
        .filter_map(|param| {
            let before_default = param.split('=').next().unwrap_or_default().trim();
            let (name, _) = before_default.split_once(':')?;
            let name = name.trim();
            (!name.is_empty() && !matches!(name, "self" | "conv" | "flow"))
                .then(|| name.to_string())
        })
        .collect()
}

pub(crate) fn python_signature_for_function(code: &str, function_name: &str) -> Option<String> {
    code.lines().find_map(|line| {
        let trimmed = line.trim();
        let signature = trimmed
            .strip_prefix("def ")
            .or_else(|| trimmed.strip_prefix("async def "))?;
        let open = signature.find('(')?;
        let name = signature[..open].trim();
        (name == function_name).then(|| signature.to_string())
    })
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

pub(crate) fn infer_function_parameters(
    code: &str,
    function_name: &str,
) -> Vec<FunctionParameterUpdate> {
    let decorator_descriptions = function_parameter_decorators(code);
    let signature = python_signature_for_function(code, function_name);
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
        .map(|(name, annotation)| {
            let parameter_path = format!("{function_name}.{name}");
            FunctionParameterUpdate {
                id: stable_resource_id("PARAMETER", name, &parameter_path),
                name: name.to_string(),
                description: decorator_descriptions
                    .get(name)
                    .cloned()
                    .unwrap_or_default(),
                r#type: annotation
                    .and_then(schema_type_from_python_annotation)
                    .unwrap_or("string")
                    .to_string(),
            }
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
