use crate::local_parse::{NonEmptyString, ResourceParseResult, deserialize_yaml, non_empty_vec};
use serde::Deserialize;
use serde_yaml_ng::Value;

#[derive(Debug, Deserialize)]
pub(crate) struct PhraseFiltersFile {
    #[serde(default)]
    pub(crate) phrase_filtering: Vec<PhraseFilterItem>,
}

pub(crate) fn parse_phrase_filters_file(
    path: &str,
    yaml: &Value,
) -> ResourceParseResult<PhraseFiltersFile> {
    deserialize_yaml(path, yaml)
}

#[derive(Debug, Deserialize)]
pub(crate) struct PhraseFilterItem {
    name: NonEmptyString,
    #[serde(default, deserialize_with = "deserialize_trimmed_string")]
    description: String,
    #[serde(deserialize_with = "regular_expressions")]
    regular_expressions: Vec<String>,
    #[serde(default)]
    say_phrase: bool,
    #[serde(default)]
    language_code: String,
    #[serde(default)]
    function: Option<String>,
}

impl PhraseFilterItem {
    pub(crate) fn name(&self) -> &str {
        self.name.as_str()
    }

    pub(crate) fn description(&self) -> &str {
        &self.description
    }

    pub(crate) fn regular_expressions(&self) -> &[String] {
        &self.regular_expressions
    }

    pub(crate) fn say_phrase(&self) -> bool {
        self.say_phrase
    }

    pub(crate) fn language_code(&self) -> &str {
        &self.language_code
    }

    pub(crate) fn function(&self) -> Option<&str> {
        self.function
            .as_deref()
            .filter(|value| !value.trim().is_empty())
    }
}

fn regular_expressions<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    non_empty_vec(deserializer, "At least one regular expression is required")
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
