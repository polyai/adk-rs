use crate::local_parse::{
    NonEmptyString, ResourceParseErrors, ResourceParseResult, deserialize_yaml,
};
use crate::{clean_name, extract_template_references};
use adk_protobuf::knowledge_base::{ExampleQueries, TopicReferences};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use serde_yaml_ng::Value as YamlValue;
use std::path::Path;

const VALID_TOPIC_REFERENCE_TYPES: &str =
    "['global_functions', 'sms', 'handoff', 'attributes', 'variables', 'translations']";

const DISALLOWED_TOPIC_REFERENCE_TYPES: &[(&str, &str)] =
    &[("transition_functions", "ft"), ("entities", "entity")];

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LocalTopic {
    name: NonEmptyString,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    actions: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    example_queries: Vec<String>,
}

impl LocalTopic {
    fn from_raw(path: &str, raw: RawTopic) -> Result<Self, String> {
        let name = raw
            .name
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| topic_name_from_path(path).unwrap_or_default());
        Ok(Self {
            name: NonEmptyString::new(name)?,
            enabled: raw.enabled,
            actions: raw.actions,
            content: raw.content,
            example_queries: raw.example_queries,
        })
    }

    fn try_from_raw(path: &str, raw: RawTopic) -> ResourceParseResult<Self> {
        let mut errors = ResourceParseErrors::new();
        let Ok(topic) = Self::from_raw(path, raw) else {
            errors.push(path, "topic name is required.");
            return Err(errors);
        };

        topic.append_validation_errors(path, &mut errors);
        if errors.is_empty() {
            Ok(topic)
        } else {
            Err(errors)
        }
    }

    pub(crate) fn from_projection(id: &str, topic: &JsonValue) -> Result<Self, String> {
        Ok(Self {
            name: NonEmptyString::new(
                topic
                    .get("name")
                    .and_then(JsonValue::as_str)
                    .unwrap_or(id)
                    .to_string(),
            )?,
            enabled: topic
                .get("isActive")
                .and_then(JsonValue::as_bool)
                .unwrap_or(true),
            actions: topic
                .get("actions")
                .and_then(JsonValue::as_str)
                .unwrap_or("")
                .to_string(),
            content: topic
                .get("content")
                .and_then(JsonValue::as_str)
                .unwrap_or("")
                .to_string(),
            example_queries: projection_example_queries(topic),
        })
    }

    pub(crate) fn name(&self) -> &str {
        self.name.as_str()
    }

    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn actions(&self) -> &str {
        &self.actions
    }

    pub(crate) fn content(&self) -> &str {
        &self.content
    }

    pub(crate) fn example_queries(&self) -> &[String] {
        &self.example_queries
    }

    pub(crate) fn example_queries_proto(&self) -> ExampleQueries {
        ExampleQueries {
            queries: self.example_queries.clone(),
        }
    }

    fn append_validation_errors(&self, path: &str, errors: &mut ResourceParseErrors) {
        if let Some(file_stem) = Path::new(path).file_stem().and_then(|stem| stem.to_str()) {
            let expected_file_name = clean_name(self.name(), true);
            if file_stem != expected_file_name {
                errors.push(
                    path,
                    format!(
                        "Topic name '{}' in file {file_stem}.yaml does not match expected filename: {expected_file_name}.yaml",
                        self.name()
                    ),
                );
            }
        }
        if self.example_queries.len() > 20 {
            errors.push(path, "Example queries must be less than 20");
        }
        append_invalid_reference_type_errors(path, self.actions(), errors);
        append_invalid_reference_type_errors(path, self.content(), errors);
    }
}

pub(crate) fn parse_topic_file(path: &str, yaml: &YamlValue) -> ResourceParseResult<LocalTopic> {
    let raw = deserialize_yaml::<RawTopic>(path, yaml)?;
    LocalTopic::try_from_raw(path, raw)
}

pub(crate) fn deserialize_topic_content(
    path: &str,
    content: &str,
) -> ResourceParseResult<Option<LocalTopic>> {
    if !path.starts_with("topics/") || !(path.ends_with(".yaml") || path.ends_with(".yml")) {
        return Ok(None);
    }
    let yaml = serde_yaml_ng::from_str::<YamlValue>(content)
        .map_err(|error| ResourceParseErrors::single(path, error))?;
    let raw = deserialize_yaml::<RawTopic>(path, &yaml)?;
    LocalTopic::from_raw(path, raw)
        .map(Some)
        .map_err(|error| ResourceParseErrors::single(path, error))
}

pub(crate) fn topic_references(actions: &str, content: &str) -> TopicReferences {
    let prompt = format!("{actions}{content}");
    let mut variables = extract_template_references(&prompt, "vrbl");
    variables.extend(extract_template_references(&prompt, "var"));
    TopicReferences {
        sms: extract_template_references(&prompt, "twilio_sms"),
        handoff: extract_template_references(&prompt, "ho"),
        attributes: extract_template_references(&prompt, "attr"),
        global_functions: extract_template_references(&prompt, "fn"),
        variables,
        translations: extract_template_references(&prompt, "tn"),
    }
}

#[derive(Debug, Deserialize)]
struct RawTopic {
    name: Option<String>,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    actions: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    example_queries: Vec<String>,
}

fn default_true() -> bool {
    true
}

fn topic_name_from_path(path: &str) -> Option<String> {
    Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(ToString::to_string)
}

fn projection_example_queries(topic: &JsonValue) -> Vec<String> {
    topic
        .get("exampleQueries")
        .and_then(JsonValue::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    item.get("query")
                        .and_then(JsonValue::as_str)
                        .or_else(|| item.as_str())
                        .map(ToString::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn append_invalid_reference_type_errors(
    path: &str,
    prompt: &str,
    errors: &mut ResourceParseErrors,
) {
    for (reference_type, prefix) in DISALLOWED_TOPIC_REFERENCE_TYPES {
        if !extract_template_references(prompt, prefix).is_empty() {
            errors.push(
                path,
                format!(
                    "Invalid reference type: {reference_type} is not a valid reference type for this resource. Valid references are: {VALID_TOPIC_REFERENCE_TYPES}"
                ),
            );
        }
    }
}
