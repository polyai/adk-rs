use crate::resource_utils::clean_name;
use adk_types::ResourceMap;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

pub(crate) const PYTHON_FUNCTION_STATUS_HASH_PREFIX: &str = "python-function:";
pub(crate) const PYTHON_FLOW_IMPORT_STATUS_KEY_PREFIX: &str = "__python_flow_import__/";

const FUNCTION_HEADER: &str = "from _gen import *  # <AUTO GENERATED>\n";
const LEGACY_FUNCTION_HEADER: &str = "from imports import *  # <AUTO GENERATED>\n";

pub(crate) fn legacy_python_function_raw(
    payload: &Value,
    include_metadata_decorators: bool,
) -> Option<String> {
    let code = payload.get("code").and_then(Value::as_str)?.to_string();
    let name = payload
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut decorators = Vec::new();
    if include_metadata_decorators
        && let Some(description) = payload.get("description").and_then(Value::as_str)
        && !description.is_empty()
    {
        decorators.push(format!(
            "@func_description({})\n",
            python_repr_string(description)
        ));
    }
    if include_metadata_decorators
        && let Some(parameters) = payload.get("parameters").and_then(Value::as_array)
    {
        for parameter in parameters {
            let parameter_name = parameter
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let description = parameter
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default();
            decorators.push(format!(
                "@func_parameter({}, {})\n",
                python_repr_string(parameter_name),
                python_repr_string(description)
            ));
        }
    }
    if let Some(latency) = payload.get("latency_control")
        && latency
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        let initial_delay = latency
            .get("initial_delay")
            .or_else(|| latency.get("initialDelay"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let interval = latency.get("interval").and_then(Value::as_i64).unwrap_or(0);
        let mut parts = vec![
            format!("delay_before_responses_start={initial_delay}"),
            format!("silence_after_each_response={interval}"),
        ];
        let delay_responses = latency
            .get("delay_responses")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|response| {
                let message = response.get("message").and_then(Value::as_str)?;
                let duration = response.get("duration").and_then(Value::as_i64)?;
                Some(format!("({}, {duration})", python_repr_string(message)))
            })
            .collect::<Vec<_>>();
        if !delay_responses.is_empty() {
            parts.push(format!("delay_responses=[{}]", delay_responses.join(", ")));
        }
        decorators.push(format!("@func_latency_control({})\n", parts.join(", ")));
    }
    Some(insert_python_function_decorators(code, name, decorators))
}

pub(crate) fn insert_python_function_decorators(
    code: String,
    function_name: &str,
    decorators: Vec<String>,
) -> String {
    if decorators.is_empty() {
        return code;
    }
    let lines = code.split_inclusive('\n').collect::<Vec<_>>();
    if lines.is_empty() {
        return code;
    }
    let target_idx = lines.iter().position(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with(&format!("def {function_name}("))
            || trimmed.starts_with(&format!("async def {function_name}("))
    });
    let Some(target_idx) = target_idx else {
        return code;
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

pub(crate) fn legacy_python_local_function_raw(
    path: &str,
    content: &str,
    snapshot_hashes: &indexmap::IndexMap<String, String>,
) -> String {
    let raw = normalize_legacy_python_flow_imports(&raw_function_content(content), snapshot_hashes);
    let include_metadata_decorators = !path.contains("/function_steps/");
    let function_name = Path::new(path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let (code, decorators) =
        extract_normalized_python_adk_decorators(&raw, include_metadata_decorators);
    insert_python_function_decorators(code, function_name, decorators)
}

pub(crate) fn normalize_python_function_metadata_spacing(content: &str) -> String {
    let lines = content.split_inclusive('\n').collect::<Vec<_>>();
    let mut out = String::new();
    for (idx, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            let previous_nonblank = lines[..idx]
                .iter()
                .rev()
                .find(|line| !line.trim().is_empty());
            let next_nonblank = lines[idx + 1..].iter().find(|line| !line.trim().is_empty());
            let before_metadata_decorator =
                next_nonblank.is_some_and(|next| next.trim_start().starts_with("@func_"));
            let after_module_docstring_before_import = previous_nonblank
                .is_some_and(|previous| closes_python_triple_quote(previous.trim()))
                && next_nonblank.is_some_and(|next| {
                    let next = next.trim_start();
                    next.starts_with("from ") || next.starts_with("import ")
                });

            if after_module_docstring_before_import {
                continue;
            }
            if before_metadata_decorator && out.ends_with("\n\n") {
                continue;
            }
        }
        out.push_str(line);
    }
    out
}

fn closes_python_triple_quote(line: &str) -> bool {
    line.ends_with("\"\"\"") || line.ends_with("'''")
}

fn normalize_legacy_python_flow_imports(
    content: &str,
    snapshot_hashes: &indexmap::IndexMap<String, String>,
) -> String {
    let mut out = content.to_string();
    for (key, flow_id) in snapshot_hashes {
        let Some(flow_folder) = key.strip_prefix(PYTHON_FLOW_IMPORT_STATUS_KEY_PREFIX) else {
            continue;
        };
        out = out.replace(
            &format!("flows.{flow_folder}.functions"),
            &format!("functions.{}", clean_name(flow_id, true)),
        );
    }
    out
}

pub(crate) fn legacy_python_snapshot_hashes(
    snapshot_hashes: &indexmap::IndexMap<String, String>,
) -> bool {
    snapshot_hashes
        .values()
        .any(|hash| hash.starts_with(PYTHON_FUNCTION_STATUS_HASH_PREFIX))
}

pub(crate) fn normalize_legacy_python_status_function_resources(
    resources: &mut ResourceMap,
    snapshot_hashes: &indexmap::IndexMap<String, String>,
) {
    for (path, resource) in resources {
        if !is_python_function_like_path(path) {
            continue;
        }
        let Some(content) = resource
            .payload
            .get("content")
            .and_then(Value::as_str)
            .map(|content| legacy_python_local_function_raw(path, content, snapshot_hashes))
        else {
            continue;
        };
        if let Some(payload) = resource.payload.as_object_mut() {
            payload.insert("content".to_string(), Value::String(content));
        }
    }
}

pub(crate) fn extract_normalized_python_adk_decorators(
    code: &str,
    include_metadata_decorators: bool,
) -> (String, Vec<String>) {
    let mut out = String::new();
    let mut decorators = Vec::new();
    let mut active: Option<(&'static str, PythonDecoratorCallScan)> = None;

    for line in code.split_inclusive('\n') {
        let trimmed = line.trim();
        if let Some((name, mut state)) = active.take() {
            state.args.push('\n');
            if state.scan(trimmed) {
                if let Some(decorator) =
                    normalize_python_adk_decorator(name, &state.args, include_metadata_decorators)
                    && !decorator.is_empty()
                {
                    decorators.push(decorator);
                }
            } else {
                active = Some((name, state));
            }
            continue;
        }

        if let Some((name, rest)) = python_adk_decorator_start(trimmed) {
            let mut state = PythonDecoratorCallScan::default();
            if state.scan(rest) {
                if let Some(decorator) =
                    normalize_python_adk_decorator(name, &state.args, include_metadata_decorators)
                    && !decorator.is_empty()
                {
                    decorators.push(decorator);
                }
            } else {
                active = Some((name, state));
            }
            continue;
        }

        out.push_str(line);
    }

    (out, decorators)
}

fn python_adk_decorator_start(line: &str) -> Option<(&'static str, &str)> {
    [
        ("func_description", "@func_description("),
        ("func_parameter", "@func_parameter("),
        ("func_latency_control", "@func_latency_control("),
    ]
    .iter()
    .find_map(|(name, prefix)| line.strip_prefix(prefix).map(|rest| (*name, rest)))
}

fn normalize_python_adk_decorator(
    name: &str,
    args: &str,
    include_metadata_decorators: bool,
) -> Option<String> {
    match name {
        "func_description" => {
            if !include_metadata_decorators {
                return Some(String::new());
            }
            let args = parse_python_string_args(args.trim().trim_end_matches(','));
            args.first().map(|description| {
                format!("@func_description({})\n", python_repr_string(description))
            })
        }
        "func_parameter" => {
            if !include_metadata_decorators {
                return Some(String::new());
            }
            let args = parse_python_string_args(args.trim().trim_end_matches(','));
            if args.len() >= 2 {
                return Some(format!(
                    "@func_parameter({}, {})\n",
                    python_repr_string(&args[0]),
                    python_repr_string(&args[1])
                ));
            }
            None
        }
        "func_latency_control" => Some(format!(
            "@func_latency_control({})\n",
            args.trim().trim_end_matches(',')
        )),
        _ => None,
    }
}

#[derive(Default)]
pub(crate) struct PythonDecoratorCallScan {
    pub(crate) args: String,
    quote: Option<char>,
    escaped: bool,
    depth: i32,
}

impl PythonDecoratorCallScan {
    pub(crate) fn scan(&mut self, fragment: &str) -> bool {
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

pub(crate) fn python_repr_string(value: &str) -> String {
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

#[derive(Debug)]
pub(crate) struct FunctionSignatureParameter {
    pub(crate) name: String,
    pub(crate) annotation: Option<String>,
}

pub(crate) fn function_signature_parameter_list(
    content: &str,
    function_name: &str,
) -> Option<Vec<FunctionSignatureParameter>> {
    let prefix = format!("def {function_name}(");
    let signature = content
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with(&prefix))?;
    let open = signature.find('(')?;
    let close = signature[open + 1..].find(')')?;
    let params = &signature[open + 1..open + 1 + close];
    Some(
        params
            .split(',')
            .map(str::trim)
            .filter(|param| !param.is_empty())
            .filter_map(|param| {
                let before_default = param.split('=').next().unwrap_or_default().trim();
                let (name, annotation) = before_default
                    .split_once(':')
                    .map(|(name, annotation)| {
                        (name.trim().to_string(), Some(annotation.trim().to_string()))
                    })
                    .unwrap_or_else(|| (before_default.to_string(), None));
                (!name.is_empty()).then_some(FunctionSignatureParameter { name, annotation })
            })
            .collect(),
    )
}

pub(crate) fn function_signature_parameters(
    content: &str,
    function_name: &str,
) -> Option<HashMap<String, Option<String>>> {
    Some(
        function_signature_parameter_list(content, function_name)?
            .into_iter()
            .map(|parameter| (parameter.name, parameter.annotation))
            .collect(),
    )
}

pub(crate) fn function_parameter_decorator_names(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("@func_parameter(")?;
            let args = parse_python_string_args(rest.strip_suffix(')').unwrap_or(rest));
            args.first().cloned()
        })
        .collect()
}

pub(crate) fn parse_python_string_args(value: &str) -> Vec<String> {
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

pub(crate) fn local_resource_content(path: &str, content: &str) -> String {
    if is_python_function_like_path(path) {
        raw_function_content(content)
    } else {
        content.to_string()
    }
}

pub(crate) fn resource_file_content(path: &str, content: &str) -> String {
    if is_python_function_like_path(path) {
        pretty_function_content(content)
    } else {
        content.to_string()
    }
}

pub(crate) fn is_python_function_resource(path: &str) -> bool {
    ((path.starts_with("functions/") && path.ends_with(".py"))
        || (path.starts_with("flows/") && path.contains("/functions/") && path.ends_with(".py")))
        && !path.contains("/function_steps/")
}

pub(crate) fn is_python_function_like_path(path: &str) -> bool {
    path.ends_with(".py")
        && ((path.starts_with("functions/"))
            || (path.starts_with("flows/")
                && (path.contains("/functions/") || path.contains("/function_steps/"))))
}

pub(crate) fn raw_function_content(content: &str) -> String {
    content
        .replace(FUNCTION_HEADER, "")
        .replace(LEGACY_FUNCTION_HEADER, "")
        .trim_start_matches('\n')
        .to_string()
}

fn pretty_function_content(content: &str) -> String {
    if content.contains(FUNCTION_HEADER) || content.contains(LEGACY_FUNCTION_HEADER) {
        return content.to_string();
    }

    let content = content.trim_start_matches('\n');
    if let Some(docstring_end) = module_docstring_end(content) {
        let before_docstring = &content[..docstring_end];
        let after_docstring = content[docstring_end..].trim_start_matches('\n');
        if after_docstring.starts_with("from ") || after_docstring.starts_with("import ") {
            format!("{before_docstring}\n{FUNCTION_HEADER}{after_docstring}")
        } else {
            format!("{before_docstring}\n{FUNCTION_HEADER}\n{after_docstring}")
        }
    } else if content.starts_with("from ") || content.starts_with("import ") {
        format!("{FUNCTION_HEADER}{content}")
    } else {
        format!("{FUNCTION_HEADER}\n\n{content}")
    }
}

fn module_docstring_end(content: &str) -> Option<usize> {
    let quote = if content.starts_with("\"\"\"") {
        "\"\"\""
    } else if content.starts_with("'''") {
        "'''"
    } else {
        return None;
    };
    content[quote.len()..]
        .find(quote)
        .map(|index| quote.len() + index + quote.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn function_file_content_is_pretty_on_disk_and_raw_in_resources() {
        let raw = "@func_description('Looks up a customer.')\ndef lookup(conv: Conversation):\n    return None\n";
        let pretty = resource_file_content("functions/lookup.py", raw);

        assert!(pretty.starts_with("from _gen import *  # <AUTO GENERATED>\n\n\n"));
        assert_eq!(local_resource_content("functions/lookup.py", &pretty), raw);
    }

    #[test]
    fn function_header_is_inserted_after_module_docstring() {
        let raw =
            "\"\"\"Helpers.\"\"\"\nimport json\n\ndef lookup(conv):\n    return json.dumps({})\n";
        let pretty = resource_file_content("functions/lookup.py", raw);

        assert!(pretty.starts_with(
            "\"\"\"Helpers.\"\"\"\nfrom _gen import *  # <AUTO GENERATED>\nimport json\n"
        ));
        assert_eq!(local_resource_content("functions/lookup.py", &pretty), raw);
    }

    #[test]
    fn legacy_function_status_hash_ignores_python_pretty_blank_churn() {
        let with_python_status_spacing = "\"\"\"Module docs.\"\"\"\n\nfrom helper import value\n\n\n@func_description('Internal helper.')\ndef helper(conv: Conversation):\n    return value\n";
        let with_local_pretty_spacing = "\"\"\"Module docs.\"\"\"\nfrom helper import value\n\n@func_description('Internal helper.')\ndef helper(conv: Conversation):\n    return value\n";

        assert_eq!(
            normalize_python_function_metadata_spacing(with_python_status_spacing),
            normalize_python_function_metadata_spacing(with_local_pretty_spacing)
        );
    }

    #[test]
    fn legacy_function_status_hash_understands_multiline_decorators() {
        let snapshot_hashes = indexmap::IndexMap::new();
        let content = "from _gen import *  # <AUTO GENERATED>\n\n@func_description(\n    'Transfers a caller.'\n)\n@func_parameter(\n    'handoff_reason',\n    'Reason copied from context.',\n)\ndef handoff(conv: Conversation, handoff_reason: str):\n    return handoff_reason\n";

        let normalized =
            legacy_python_local_function_raw("functions/handoff.py", content, &snapshot_hashes);

        assert_eq!(
            normalized,
            "@func_description('Transfers a caller.')\n@func_parameter('handoff_reason', 'Reason copied from context.')\ndef handoff(conv: Conversation, handoff_reason: str):\n    return handoff_reason\n"
        );
    }
}
