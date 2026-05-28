use crate::clean_name;
use crate::functions::{
    PYTHON_FUNCTION_STATUS_HASH_PREFIX, PythonDecoratorCallScan,
    extract_normalized_python_adk_decorators, function_signature_parameter_list,
    insert_python_function_decorators, is_python_function_like_path, legacy_python_function_raw,
    legacy_python_local_function_raw, normalize_python_function_metadata_spacing,
    parse_python_string_args, raw_function_content,
};
use adk_io::{FileSystem, compute_hash, parse_multi_resource_path};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

/// Typed envelope for Python's base64-encoded `_gen/.agent_studio_config`.
///
/// Resource payloads intentionally stay open-ended so Rust can round-trip
/// Python-authored status files without needing to know every resource field.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct StatusSnapshot {
    #[serde(default, deserialize_with = "default_if_null")]
    pub region: String,
    #[serde(default, deserialize_with = "default_if_null")]
    pub account_id: String,
    #[serde(default, deserialize_with = "default_if_null")]
    pub project_id: String,
    #[serde(default)]
    pub project_name: Option<String>,
    #[serde(default, deserialize_with = "default_if_null")]
    pub resources: IndexMap<String, IndexMap<String, StatusResourcePayload>>,
    #[serde(default)]
    pub last_updated: Option<String>,
    #[serde(default, deserialize_with = "default_if_null")]
    pub file_structure_info: IndexMap<String, FileStructureEntry>,
    #[serde(
        default = "default_branch",
        deserialize_with = "default_branch_if_null"
    )]
    pub branch_id: String,
    #[serde(default, deserialize_with = "default_if_null")]
    pub migration_flags: Vec<String>,
    #[serde(default, flatten)]
    pub extra: Map<String, Value>,
}

fn default_branch() -> String {
    "main".to_string()
}

fn default_if_null<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

fn default_branch_if_null<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_else(default_branch))
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct StatusResourcePayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(default, flatten)]
    pub fields: Map<String, Value>,
}

impl StatusResourcePayload {
    pub fn from_value(value: Value) -> Self {
        serde_json::from_value(value).unwrap_or_default()
    }

    pub fn as_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn resource_id(&self) -> Option<&str> {
        self.resource_id.as_deref()
    }

    pub fn file_path(&self) -> Option<&str> {
        self.file_path.as_deref()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FileStructureEntry {
    #[serde(default, rename = "type")]
    pub resource_type: String,
    #[serde(default)]
    pub resource_id: String,
    #[serde(default)]
    pub resource_name: String,
    #[serde(default)]
    pub hash: String,
    #[serde(default, flatten)]
    pub extra: Map<String, Value>,
}

pub struct ResourceStatusPayloadInput<'a> {
    pub type_name: &'a str,
    pub logical_path: &'a str,
    pub content: &'a str,
    pub resource_id: &'a str,
    pub fallback_name: &'a str,
    pub variant_name_to_id: &'a BTreeMap<String, String>,
    pub flow_step_name_to_id: &'a BTreeMap<(String, String), String>,
}

pub fn legacy_python_status_resource_path(
    resource_name: &str,
    payload: &Value,
    ordinal: usize,
) -> Option<String> {
    let name = payload
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let clean_status_name = |lowercase| clean_name(name, lowercase);
    let flow_folder = || {
        payload
            .get("flow_name")
            .and_then(Value::as_str)
            .map(|flow_name| clean_name(flow_name, true))
    };
    match resource_name {
        "api_integration" => Some(format!(
            "config/api_integrations.yaml/api_integrations/{}",
            clean_status_name(false)
        )),
        "functions" => {
            if let Some(flow_folder) = flow_folder() {
                Some(format!("flows/{flow_folder}/functions/{name}.py"))
            } else {
                Some(format!("functions/{name}.py"))
            }
        }
        "topics" => Some(format!("topics/{}.yaml", clean_status_name(true))),
        "personality" => Some("agent_settings/personality.yaml".to_string()),
        "role" => Some("agent_settings/role.yaml".to_string()),
        "rules" => Some("agent_settings/rules.txt".to_string()),
        "flow_steps" => flow_folder().map(|flow_folder| {
            format!("flows/{flow_folder}/steps/{}.yaml", clean_status_name(true))
        }),
        "function_steps" => {
            flow_folder().map(|flow_folder| format!("flows/{flow_folder}/function_steps/{name}.py"))
        }
        "flow_config" => Some(format!(
            "flows/{}/flow_config.yaml",
            clean_status_name(true)
        )),
        "entities" => Some(format!(
            "config/entities.yaml/entities/{}",
            clean_status_name(false)
        )),
        "experimental_config" => Some("agent_settings/experimental_config.json".to_string()),
        "safety_filters" => Some("agent_settings/safety_filters.yaml".to_string()),
        "sms_templates" => Some(format!(
            "config/sms_templates.yaml/sms_templates/{}",
            clean_status_name(false)
        )),
        "handoffs" => Some(format!(
            "config/handoffs.yaml/handoffs/{}",
            clean_status_name(false)
        )),
        "variants" => Some(format!(
            "config/variant_attributes.yaml/variants/{}",
            clean_status_name(false)
        )),
        "variant_attributes" => Some(format!(
            "config/variant_attributes.yaml/attributes/{}",
            clean_status_name(false)
        )),
        "variables" => Some(format!("variables/{name}")),
        "voice_greeting" => Some("voice/configuration.yaml/greeting".to_string()),
        "voice_safety_filters" => Some("voice/safety_filters.yaml".to_string()),
        "voice_style_prompt" => Some("voice/configuration.yaml/style_prompt".to_string()),
        "voice_disclaimer" => Some("voice/configuration.yaml/disclaimer_messages".to_string()),
        "chat_greeting" => Some("chat/configuration.yaml/greeting".to_string()),
        "chat_safety_filters" => Some("chat/safety_filters.yaml".to_string()),
        "chat_style_prompt" => Some("chat/configuration.yaml/style_prompt".to_string()),
        "keyphrase_boosting" => {
            let keyphrase = payload
                .get("keyphrase")
                .and_then(Value::as_str)
                .unwrap_or(name);
            Some(format!(
                "voice/speech_recognition/keyphrase_boosting.yaml/keyphrases/{}",
                clean_name(keyphrase, false)
            ))
        }
        "transcript_corrections" => Some(format!(
            "voice/speech_recognition/transcript_corrections.yaml/corrections/{}",
            clean_status_name(false)
        )),
        "asr_settings" => Some("voice/speech_recognition/asr_settings.yaml".to_string()),
        "phrase_filtering" => Some(format!(
            "voice/response_control/phrase_filtering.yaml/phrase_filtering/{}",
            clean_status_name(false)
        )),
        "pronunciations" => {
            let position = payload
                .get("position")
                .and_then(Value::as_i64)
                .map(|value| value.to_string())
                .unwrap_or_else(|| ordinal.to_string());
            Some(format!(
                "voice/response_control/pronunciations.yaml/pronunciations/{}",
                clean_name(&position, false)
            ))
        }
        _ => None,
    }
}

pub fn legacy_python_rules_reference_names(
    resources: &IndexMap<String, IndexMap<String, StatusResourcePayload>>,
) -> Vec<(String, String, String)> {
    [
        ("functions", "fn"),
        ("sms_templates", "sms"),
        ("handoffs", "handoff"),
        ("variant_attributes", "attr"),
        ("variables", "vrbl"),
    ]
    .into_iter()
    .flat_map(|(resource_name, reference_prefix)| {
        resources
            .get(resource_name)
            .into_iter()
            .flat_map(move |entries| {
                entries.values().filter_map(move |payload| {
                    let id = payload.resource_id()?;
                    let name = payload.name()?;
                    Some((
                        reference_prefix.to_string(),
                        id.to_string(),
                        name.to_string(),
                    ))
                })
            })
    })
    .collect()
}

fn replace_resource_ids_with_names(
    content: &str,
    replacements: &[(String, String, String)],
) -> String {
    let mut normalized = content.to_string();
    for (prefix, id, name) in replacements {
        if id.is_empty() || id == name {
            continue;
        }
        normalized = normalized.replace(
            &format!("{{{{{prefix}:{id}}}}}"),
            &format!("{{{{{prefix}:{name}}}}}"),
        );
    }
    normalized
}

pub fn legacy_python_status_resource_file_hash<Fs: FileSystem>(
    fs: &Fs,
    root: &Path,
    resource_name: &str,
    file_path: &str,
    payload: &Value,
    rules_reference_names: &[(String, String, String)],
) -> Option<String> {
    if payload.get("file_path").is_some() {
        return None;
    }
    match resource_name {
        "functions" => {
            let raw = legacy_python_function_raw(payload, true)?;
            Some(format!(
                "{PYTHON_FUNCTION_STATUS_HASH_PREFIX}{}",
                compute_hash(&normalize_python_function_metadata_spacing(&raw))
            ))
        }
        "function_steps" => {
            let raw = legacy_python_function_raw(payload, false)?;
            Some(format!(
                "{PYTHON_FUNCTION_STATUS_HASH_PREFIX}{}",
                compute_hash(&normalize_python_function_metadata_spacing(&raw))
            ))
        }
        "rules" => payload
            .get("behaviour")
            .and_then(Value::as_str)
            .map(|raw| compute_hash(&replace_resource_ids_with_names(raw, rules_reference_names))),
        "variables" => payload
            .get("name")
            .and_then(Value::as_str)
            .map(|name| compute_hash(&format!("vrbl:{name}"))),
        _ => fs
            .read_to_string(&root.join(file_path))
            .ok()
            .map(|content| compute_hash(&content)),
    }
}

pub fn legacy_python_status_resource_content(
    resource_name: &str,
    payload: &Value,
) -> Option<String> {
    match resource_name {
        "functions" => legacy_python_function_raw(payload, true),
        "function_steps" => legacy_python_function_raw(payload, false),
        "rules" => payload
            .get("behaviour")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        _ => None,
    }
}

pub fn resource_status_payload(input: ResourceStatusPayloadInput<'_>) -> Value {
    let mut payload = match input.type_name {
        "Function" => {
            status_function_payload(input.logical_path, input.content, input.fallback_name)
        }
        "FunctionStep" => {
            status_function_step_payload(input.logical_path, input.content, input.fallback_name)
        }
        "FlowConfig" => status_flow_config_payload(
            input.logical_path,
            input.content,
            input.flow_step_name_to_id,
        ),
        "FlowStep" => {
            status_flow_step_payload(input.logical_path, input.content, input.fallback_name)
        }
        "SettingsRules" => serde_json::json!({
            "name": "rules",
            "behaviour": input.content,
        }),
        "ExperimentalConfig" => serde_json::json!({
            "name": "experimental_config",
            "config": serde_json::from_str::<Value>(input.content).unwrap_or_else(|_| serde_json::json!({})),
        }),
        "GeneralSafetyFilters" => {
            status_safety_filters_payload(input.logical_path, input.content, false)
        }
        "VoiceSafetyFilters" | "ChatSafetyFilters" => {
            status_safety_filters_payload(input.logical_path, input.content, true)
        }
        "Pronunciation" => {
            status_pronunciation_payload(input.logical_path, input.content, input.fallback_name)
        }
        "VariantAttribute" => status_variant_attribute_payload(
            input.logical_path,
            input.content,
            input.fallback_name,
            input.variant_name_to_id,
        ),
        "Variable" => serde_json::json!({
            "name": input
                .logical_path
                .strip_prefix("variables/")
                .unwrap_or(input.fallback_name),
            "references": {},
        }),
        _ => status_yaml_payload(input.logical_path, input.content)
            .unwrap_or_else(|| serde_json::json!({ "name": input.fallback_name })),
    };
    snake_case_json_keys(&mut payload);
    let Some(object) = payload.as_object_mut() else {
        payload = serde_json::json!({ "name": input.fallback_name });
        let object = payload.as_object_mut().expect("payload object");
        object.insert(
            "resource_id".to_string(),
            Value::String(input.resource_id.to_string()),
        );
        object.insert(
            "file_path".to_string(),
            Value::String(input.logical_path.to_string()),
        );
        return payload;
    };
    object
        .entry("name".to_string())
        .or_insert_with(|| Value::String(input.fallback_name.to_string()));
    object.insert(
        "resource_id".to_string(),
        Value::String(input.resource_id.to_string()),
    );
    object.insert(
        "file_path".to_string(),
        Value::String(input.logical_path.to_string()),
    );
    payload
}

pub fn resource_status_file_hash(
    type_name: &str,
    logical_path: &str,
    content: &str,
    payload: &Value,
    variant_name_to_id: &BTreeMap<String, String>,
) -> String {
    if let Some(name) = logical_path.strip_prefix("variables/") {
        return compute_hash(&format!("vrbl:{name}"));
    }
    match type_name {
        "Function" => {
            let raw = legacy_python_function_raw(payload, true).unwrap_or_default();
            compute_hash(&raw)
        }
        "FunctionStep" => {
            let raw = legacy_python_function_raw(payload, false).unwrap_or_default();
            compute_hash(&raw)
        }
        "SettingsRules" => compute_hash(
            payload
                .get("behaviour")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ),
        "ExperimentalConfig" => {
            let config = payload.get("config").unwrap_or(&Value::Null);
            compute_hash(&python_json_dumps_pretty_sorted(config))
        }
        "Pronunciation" => compute_hash(&python_json_dumps_sorted(
            &status_pronunciation_hash_payload(payload),
        )),
        "VariantAttribute" => {
            let value = status_yaml_payload(logical_path, content).unwrap_or(Value::Null);
            compute_hash(&python_json_dumps_sorted(
                &status_variant_attribute_hash_payload(&value, variant_name_to_id),
            ))
        }
        _ => {
            if let Some(value) = status_yaml_payload(logical_path, content) {
                compute_hash(&python_json_dumps_sorted(&value))
            } else {
                compute_hash(content)
            }
        }
    }
}

pub fn status_function_payload(logical_path: &str, content: &str, fallback_name: &str) -> Value {
    let name = path_stem(logical_path).unwrap_or(fallback_name).to_string();
    let flow_name = flow_folder_name(logical_path);
    let raw_code = raw_function_content(content);
    let description = status_function_description(&raw_code);
    let parameters = status_function_parameters(&raw_code, &name);
    let code = status_function_code_without_metadata_decorators(&raw_code, &name);
    let function_type = if logical_path.starts_with("flows/") {
        "transition"
    } else if logical_path == "functions/start_function.py" {
        "start"
    } else if logical_path == "functions/end_function.py" {
        "end"
    } else {
        "global"
    };
    let mut payload = serde_json::json!({
        "name": name,
        "description": description,
        "code": code,
        "parameters": parameters,
        "latency_control": {},
        "function_type": function_type,
        "variable_references": {},
    });
    if let Some(flow_name) = flow_name {
        payload["flow_name"] = Value::String(flow_name);
    }
    payload
}

fn status_function_code_without_metadata_decorators(code: &str, function_name: &str) -> String {
    let (code, decorators) = extract_normalized_python_adk_decorators(code, false);
    insert_python_function_decorators(code, function_name, decorators)
}

fn status_function_description(code: &str) -> String {
    if let Some(description) = python_decorator_args(code, "func_description")
        .into_iter()
        .find_map(|args| {
            parse_python_string_args(args.trim().trim_end_matches(','))
                .into_iter()
                .next()
        })
    {
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

fn status_function_parameters(code: &str, function_name: &str) -> Vec<Value> {
    let decorator_descriptions = python_decorator_args(code, "func_parameter")
        .into_iter()
        .filter_map(|args| {
            let args = parse_python_string_args(args.trim().trim_end_matches(','));
            (args.len() >= 2).then(|| (args[0].clone(), args[1].clone()))
        })
        .collect::<HashMap<_, _>>();
    let Some(parameters) = function_signature_parameter_list(code, function_name) else {
        return vec![];
    };
    parameters
        .into_iter()
        .filter(|parameter| !matches!(parameter.name.as_str(), "self" | "conv" | "flow"))
        .map(|parameter| {
            let name = parameter.name;
            let parameter_type = parameter
                .annotation
                .as_deref()
                .and_then(status_schema_type_from_python_annotation)
                .unwrap_or("string");
            let parameter_path = format!("{function_name}.{name}");
            let id = stable_status_resource_id("PARAMETER", &name, &parameter_path);
            let description = decorator_descriptions
                .get(&name)
                .cloned()
                .unwrap_or_default();
            serde_json::json!({
                "id": id,
                "name": name,
                "description": description,
                "type": parameter_type,
            })
        })
        .collect()
}

fn python_decorator_args(code: &str, decorator_name: &str) -> Vec<String> {
    let prefix = format!("@{decorator_name}(");
    let mut calls = Vec::new();
    let mut active: Option<PythonDecoratorCallScan> = None;
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
        let mut state = PythonDecoratorCallScan::default();
        if state.scan(rest) {
            calls.push(state.args.trim().trim_end_matches(',').to_string());
        } else {
            active = Some(state);
        }
    }
    calls
}

fn status_schema_type_from_python_annotation(annotation: &str) -> Option<&'static str> {
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

fn stable_status_resource_id(prefix: &str, name: &str, path: &str) -> String {
    let digest = compute_hash(&format!("{name}\0{path}"));
    format!("{prefix}-{}", &digest[..16])
}

pub fn status_function_step_payload(
    logical_path: &str,
    content: &str,
    fallback_name: &str,
) -> Value {
    let name = path_stem(logical_path).unwrap_or(fallback_name).to_string();
    let flow_name = flow_folder_name(logical_path).unwrap_or_default();
    serde_json::json!({
        "name": name,
        "step_id": "",
        "flow_id": "",
        "flow_name": flow_name,
        "code": raw_function_content(content),
        "description": null,
        "parameters": [],
        "latency_control": {},
        "position": {},
        "function_id": "",
        "variable_references": {},
    })
}

pub fn status_flow_config_payload(
    logical_path: &str,
    content: &str,
    flow_step_name_to_id: &BTreeMap<(String, String), String>,
) -> Value {
    let mut payload =
        status_yaml_payload(logical_path, content).unwrap_or_else(|| serde_json::json!({}));
    let Some(object) = payload.as_object_mut() else {
        return payload;
    };
    let Some(folder) = flow_folder_name(logical_path) else {
        return payload;
    };
    let Some(start_step) = object.get("start_step").and_then(Value::as_str) else {
        return payload;
    };
    if let Some(id) = flow_step_name_to_id.get(&(folder, start_step.to_string())) {
        let flow_name = object
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let normalized_id = id
            .strip_prefix(&format!("{flow_name}_"))
            .unwrap_or(id)
            .to_string();
        object.insert("start_step".to_string(), Value::String(normalized_id));
    }
    payload
}

pub fn status_flow_step_payload(logical_path: &str, content: &str, fallback_name: &str) -> Value {
    let mut payload =
        status_yaml_payload(logical_path, content).unwrap_or_else(|| serde_json::json!({}));
    let flow_name = flow_folder_name(logical_path).unwrap_or_default();
    let name = payload
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(fallback_name)
        .to_string();
    let Some(object) = payload.as_object_mut() else {
        return serde_json::json!({
            "name": name,
            "step_id": "",
            "flow_id": "",
            "flow_name": flow_name,
            "step_type": "advanced_step",
            "prompt": "",
        });
    };
    object
        .entry("name".to_string())
        .or_insert_with(|| Value::String(name));
    object
        .entry("step_id".to_string())
        .or_insert_with(|| Value::String(String::new()));
    object
        .entry("flow_id".to_string())
        .or_insert_with(|| Value::String(String::new()));
    object
        .entry("flow_name".to_string())
        .or_insert_with(|| Value::String(flow_name));
    object
        .entry("step_type".to_string())
        .or_insert_with(|| Value::String("advanced_step".to_string()));
    object
        .entry("prompt".to_string())
        .or_insert_with(|| Value::String(String::new()));
    payload
}

pub fn status_variant_attribute_payload(
    logical_path: &str,
    content: &str,
    fallback_name: &str,
    variant_name_to_id: &BTreeMap<String, String>,
) -> Value {
    let mut payload =
        status_yaml_payload(logical_path, content).unwrap_or_else(|| serde_json::json!({}));
    let Some(object) = payload.as_object_mut() else {
        return serde_json::json!({
            "name": fallback_name,
            "mappings": {},
        });
    };
    if !object.contains_key("mappings") {
        let mappings = object
            .remove("values")
            .map(|value| status_variant_attribute_values_to_ids(value, variant_name_to_id))
            .unwrap_or_else(|| serde_json::json!({}));
        object.insert("mappings".to_string(), mappings);
    }
    payload
}

fn status_variant_attribute_values_to_ids(
    value: Value,
    variant_name_to_id: &BTreeMap<String, String>,
) -> Value {
    let Some(values) = value.as_object() else {
        return value;
    };
    let mut mapped = serde_json::Map::new();
    for (key, value) in values {
        let key = variant_name_to_id.get(key).unwrap_or(key).clone();
        mapped.insert(key, value.clone());
    }
    Value::Object(mapped)
}

pub fn status_pronunciation_payload(
    logical_path: &str,
    content: &str,
    fallback_name: &str,
) -> Value {
    let mut payload =
        status_yaml_payload(logical_path, content).unwrap_or_else(|| serde_json::json!({}));
    let position = logical_path
        .split('/')
        .next_back()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0);
    let Some(object) = payload.as_object_mut() else {
        return serde_json::json!({
            "name": "",
            "position": position,
        });
    };
    object
        .entry("name".to_string())
        .or_insert_with(|| Value::String(String::new()));
    object.insert("position".to_string(), Value::Number(position.into()));
    if object
        .get("name")
        .and_then(Value::as_str)
        .is_some_and(|name| name == fallback_name)
    {
        object.insert("name".to_string(), Value::String(String::new()));
    }
    payload
}

pub fn status_safety_filters_payload(
    logical_path: &str,
    content: &str,
    include_enabled: bool,
) -> Value {
    let yaml = status_yaml_payload(logical_path, content).unwrap_or_else(|| serde_json::json!({}));
    let mut payload = serde_json::Map::new();
    if include_enabled {
        payload.insert(
            "enabled".to_string(),
            yaml.get("enabled").cloned().unwrap_or(Value::Bool(true)),
        );
    }
    let mut categories = serde_json::Map::new();
    for key in ["violence", "hate", "sexual", "self_harm"] {
        let category = yaml
            .get("categories")
            .and_then(|categories| categories.get(key))
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        categories.insert(key.to_string(), status_safety_filter_category(category));
    }
    payload.insert("categories".to_string(), Value::Object(categories));
    Value::Object(payload)
}

fn status_safety_filter_category(category: Value) -> Value {
    serde_json::json!({
        "enabled": category.get("enabled").cloned().unwrap_or(Value::Null),
        "precision": safety_filter_level_to_precision(
            category
                .get("level")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ),
    })
}

fn safety_filter_level_to_precision(level: &str) -> String {
    match level {
        "lenient" => "LOOSE".to_string(),
        "medium" => "MEDIUM".to_string(),
        "strict" => "STRICT".to_string(),
        value => value.to_string(),
    }
}

pub fn status_pronunciation_hash_payload(payload: &Value) -> Value {
    let mut object = serde_json::Map::new();
    for key in [
        "regex",
        "replacement",
        "case_sensitive",
        "language_code",
        "description",
    ] {
        let Some(value) = payload.get(key) else {
            continue;
        };
        if key != "replacement" && value.as_str() == Some("") {
            continue;
        }
        object.insert(key.to_string(), value.clone());
    }
    Value::Object(object)
}

pub fn status_variant_attribute_hash_payload(
    payload: &Value,
    variant_name_to_id: &BTreeMap<String, String>,
) -> Value {
    let mut object = serde_json::Map::new();
    if let Some(name) = payload.get("name") {
        object.insert("name".to_string(), name.clone());
    }
    let values = payload
        .get("values")
        .or_else(|| payload.get("mappings"))
        .and_then(Value::as_object)
        .map(|values| {
            let mut mapped = serde_json::Map::new();
            for (key, value) in values {
                let key = variant_name_to_id.get(key).unwrap_or(key).clone();
                mapped.insert(key, value.clone());
            }
            Value::Object(mapped)
        })
        .unwrap_or_else(|| serde_json::json!({}));
    object.insert("values".to_string(), values);
    Value::Object(object)
}

pub fn status_yaml_payload(logical_path: &str, content: &str) -> Option<Value> {
    let yaml = serde_yaml::from_str::<serde_yaml::Value>(content).ok()?;
    let value = if let (_, Some(suffix)) = parse_multi_resource_path(logical_path) {
        let mut segments = suffix.split('/');
        let top_level_name = segments.next()?;
        let resource_name = segments.next_back();
        let top = yaml.get(top_level_name)?;
        if let Some(resource_name) = resource_name {
            if let Some(items) = top.as_sequence() {
                if top_level_name == "pronunciations"
                    && let Ok(index) = resource_name.parse::<usize>()
                {
                    return serde_json::to_value(items.get(index)?.clone()).ok();
                }
                items
                    .iter()
                    .find(|item| {
                        item.get("name")
                            .and_then(serde_yaml::Value::as_str)
                            .is_some_and(|name| clean_name(name, false) == resource_name)
                    })
                    .cloned()?
            } else {
                top.clone()
            }
        } else {
            top.clone()
        }
    } else {
        yaml
    };
    serde_json::to_value(value).ok()
}

pub fn python_json_dumps_sorted(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => serde_json::to_string(value).unwrap_or_default(),
        Value::Array(items) => {
            let items = items
                .iter()
                .map(python_json_dumps_sorted)
                .collect::<Vec<_>>();
            format!("[{}]", items.join(", "))
        }
        Value::Object(object) => {
            let mut entries = object.iter().collect::<Vec<_>>();
            entries.sort_by_key(|(key, _)| *key);
            let entries = entries
                .into_iter()
                .map(|(key, value)| {
                    format!(
                        "{}: {}",
                        serde_json::to_string(key).unwrap_or_default(),
                        python_json_dumps_sorted(value)
                    )
                })
                .collect::<Vec<_>>();
            format!("{{{}}}", entries.join(", "))
        }
    }
}

pub fn python_json_dumps_pretty_sorted(value: &Value) -> String {
    let sorted = sort_json_value(value);
    serde_json::to_string_pretty(&sorted).unwrap_or_default()
}

fn sort_json_value(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(sort_json_value).collect()),
        Value::Object(object) => {
            let mut sorted = serde_json::Map::new();
            let mut keys = object.keys().collect::<Vec<_>>();
            keys.sort();
            for key in keys {
                if let Some(value) = object.get(key) {
                    sorted.insert(key.clone(), sort_json_value(value));
                }
            }
            Value::Object(sorted)
        }
        value => value.clone(),
    }
}

pub fn snake_case_json_keys(value: &mut Value) {
    snake_case_json_keys_inner(value, None, false);
}

fn snake_case_json_keys_inner(value: &mut Value, parent_key: Option<&str>, preserve_tree: bool) {
    match value {
        Value::Object(object) => {
            let preserve_keys = preserve_tree
                || matches!(
                    parent_key,
                    Some(
                        "adjectives"
                            | "attributes"
                            | "config"
                            | "mappings"
                            | "references"
                            | "topics"
                            | "translations"
                            | "values"
                            | "variable_references"
                            | "variables"
                            | "variants"
                    )
                );
            let old = std::mem::take(object);
            for (key, mut value) in old {
                let child_preserve_tree = preserve_tree
                    || matches!(
                        key.as_str(),
                        "adjectives"
                            | "attributes"
                            | "config"
                            | "mappings"
                            | "references"
                            | "topics"
                            | "translations"
                            | "values"
                            | "variable_references"
                            | "variables"
                            | "variants"
                    );
                snake_case_json_keys_inner(&mut value, Some(&key), child_preserve_tree);
                let key = if preserve_keys {
                    key
                } else {
                    camel_to_snake(&key)
                };
                object.insert(key, value);
            }
        }
        Value::Array(items) => {
            for item in items {
                snake_case_json_keys_inner(item, parent_key, preserve_tree);
            }
        }
        _ => {}
    }
}

fn camel_to_snake(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for (idx, ch) in value.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if idx > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn path_stem(path: &str) -> Option<&str> {
    Path::new(path).file_stem().and_then(|value| value.to_str())
}

pub fn flow_folder_name(path: &str) -> Option<String> {
    let mut parts = path.split('/');
    while let Some(part) = parts.next() {
        if part == "flows" {
            return parts.next().map(ToString::to_string);
        }
    }
    None
}

pub fn current_status_hash_for_expected(
    path: &str,
    content: &str,
    expected_hash: &str,
    snapshot_hashes: &IndexMap<String, String>,
) -> String {
    let raw_hash = compute_hash(content);
    if raw_hash == expected_hash {
        return raw_hash;
    }
    if is_python_function_like_path(path) {
        let raw_function_hash = compute_hash(&raw_function_content(content));
        if raw_function_hash == expected_hash {
            return raw_function_hash;
        }
        let prefixed_raw_function_hash =
            format!("{PYTHON_FUNCTION_STATUS_HASH_PREFIX}{raw_function_hash}");
        if prefixed_raw_function_hash == expected_hash {
            return prefixed_raw_function_hash;
        }
        let hash = compute_hash(&legacy_python_local_function_raw(
            path,
            content,
            snapshot_hashes,
        ));
        if expected_hash
            .strip_prefix(PYTHON_FUNCTION_STATUS_HASH_PREFIX)
            .is_some()
        {
            let normalized = legacy_python_local_function_raw(path, content, snapshot_hashes);
            let normalized_hash =
                compute_hash(&normalize_python_function_metadata_spacing(&normalized));
            return format!("{PYTHON_FUNCTION_STATUS_HASH_PREFIX}{normalized_hash}");
        }
        return hash;
    }
    if let Some(name) = path.strip_prefix("variables/") {
        return compute_hash(&format!("vrbl:{name}"));
    }
    if path == "agent_settings/experimental_config.json"
        && let Ok(value) = serde_json::from_str::<Value>(content)
    {
        return compute_hash(&python_json_dumps_pretty_sorted(&value));
    }
    if path.ends_with(".yaml") || path.contains(".yaml/") {
        let value = if path.contains("/pronunciations/") {
            status_pronunciation_hash_payload(&status_pronunciation_payload(path, content, ""))
        } else if path.contains("variant_attributes.yaml/attributes/") {
            status_yaml_payload(path, content)
                .map(|value| {
                    status_variant_attribute_hash_payload(
                        &value,
                        &variant_name_to_id_from_snapshot_hashes(snapshot_hashes),
                    )
                })
                .unwrap_or(Value::Null)
        } else {
            status_yaml_payload(path, content).unwrap_or(Value::Null)
        };
        if !value.is_null() {
            return compute_hash(&python_json_dumps_sorted(&value));
        }
    }
    raw_hash
}

fn variant_name_to_id_from_snapshot_hashes(
    snapshot_hashes: &IndexMap<String, String>,
) -> BTreeMap<String, String> {
    snapshot_hashes
        .iter()
        .filter_map(|(key, value)| {
            key.strip_prefix(crate::PYTHON_VARIANT_STATUS_KEY_PREFIX)
                .map(|name| (name.to_string(), value.clone()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_resource_ids_use_sha256_digest_prefix() {
        assert_eq!(
            stable_status_resource_id("PREFIX", "Name", "path/file.py"),
            "PREFIX-a563c77770234e1b"
        );
    }

    #[test]
    fn status_snapshot_preserves_unknown_top_level_and_payload_fields() {
        let raw = serde_json::json!({
            "region": "eu-west-1",
            "account_id": "acct",
            "project_id": "proj",
            "resources": {
                "functions": {
                    "fn-1": {
                        "name": "Lookup",
                        "resource_id": "fn-1",
                        "file_path": "functions/lookup.py",
                        "code": "def lookup(conv):\n    return None\n",
                        "python_only": {"still": true}
                    }
                }
            },
            "file_structure_info": {
                "functions/lookup.py": {
                    "type": "functions",
                    "resource_id": "fn-1",
                    "resource_name": "Lookup",
                    "hash": "abc",
                    "python_only": 7
                }
            },
            "future_python_field": "kept"
        });

        let snapshot: StatusSnapshot = serde_json::from_value(raw).expect("typed status snapshot");
        let encoded = serde_json::to_value(&snapshot).expect("serialize status snapshot");

        assert_eq!(encoded["future_python_field"], "kept");
        assert_eq!(
            encoded["resources"]["functions"]["fn-1"]["python_only"]["still"],
            true
        );
        assert_eq!(
            encoded["file_structure_info"]["functions/lookup.py"]["python_only"],
            7
        );
    }
}
