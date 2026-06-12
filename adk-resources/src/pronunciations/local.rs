use crate::local_parse::{NonEmptyString, ResourceParseResult, deserialize_yaml};
use serde::Deserialize;
use serde_yaml_ng::Value;

#[derive(Debug, Deserialize)]
pub(crate) struct PronunciationsFile {
    #[serde(default)]
    pub(crate) pronunciations: Vec<PronunciationItem>,
}

pub(crate) fn parse_pronunciations_file(
    path: &str,
    yaml: &Value,
) -> ResourceParseResult<PronunciationsFile> {
    deserialize_yaml(path, yaml)
}

#[derive(Debug, Deserialize)]
pub(crate) struct PronunciationItem {
    regex: NonEmptyString,
    #[serde(default)]
    replacement: String,
    #[serde(default)]
    case_sensitive: bool,
    #[serde(default)]
    language_code: String,
    #[serde(default, deserialize_with = "deserialize_trimmed_string")]
    description: String,
    #[serde(default)]
    name: String,
}

impl PronunciationItem {
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
