use crate::local_parse::{NonEmptyString, ResourceParseResult, deserialize_yaml};
use serde::{Deserialize, Serialize};
use serde_yaml_ng::Value;

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct PronunciationsFile {
    #[serde(default)]
    pub(crate) pronunciations: Vec<PronunciationItem>,
}

impl PronunciationsFile {
    pub(crate) fn new(pronunciations: Vec<PronunciationItem>) -> Self {
        Self { pronunciations }
    }
}

pub(crate) fn parse_pronunciations_file(
    path: &str,
    yaml: &Value,
) -> ResourceParseResult<PronunciationsFile> {
    deserialize_yaml(path, yaml)
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct PronunciationItem {
    regex: NonEmptyString,
    #[serde(default)]
    replacement: String,
    #[serde(default)]
    case_sensitive: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    language_code: String,
    #[serde(
        default,
        deserialize_with = "deserialize_trimmed_string",
        skip_serializing_if = "String::is_empty"
    )]
    description: String,
    #[serde(default, skip_serializing)]
    name: String,
}

impl PronunciationItem {
    pub(crate) fn new(
        regex: String,
        replacement: String,
        case_sensitive: bool,
        language_code: String,
        description: String,
        name: String,
    ) -> Result<Self, String> {
        Ok(Self {
            regex: NonEmptyString::new(regex)?,
            replacement,
            case_sensitive,
            language_code,
            description: description.trim().to_string(),
            name,
        })
    }

    pub(crate) fn regex(&self) -> &str {
        self.regex.as_str()
    }

    pub(crate) fn replacement(&self) -> &str {
        &self.replacement
    }

    pub(crate) fn case_sensitive(&self) -> bool {
        self.case_sensitive
    }

    pub(crate) fn language_code(&self) -> &str {
        &self.language_code
    }

    pub(crate) fn description(&self) -> &str {
        &self.description
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }
}

fn deserialize_trimmed_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?
        .unwrap_or_default()
        .trim()
        .to_string())
}
