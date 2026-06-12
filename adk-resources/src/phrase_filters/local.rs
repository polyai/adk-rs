use crate::local_parse::{NonEmptyString, ResourceParseResult, deserialize_yaml, non_empty_vec};
use serde::{Deserialize, Serialize};
use serde_yaml_ng::Value;

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct PhraseFiltersFile {
    #[serde(default)]
    pub(crate) phrase_filtering: Vec<PhraseFilterItem>,
}

impl PhraseFiltersFile {
    pub(crate) fn new(phrase_filtering: Vec<PhraseFilterItem>) -> Self {
        Self { phrase_filtering }
    }
}

pub(crate) fn parse_phrase_filters_file(
    path: &str,
    yaml: &Value,
) -> ResourceParseResult<PhraseFiltersFile> {
    deserialize_yaml(path, yaml)
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct PhraseFilterItem {
    name: NonEmptyString,
    #[serde(
        default,
        deserialize_with = "deserialize_trimmed_string",
        skip_serializing_if = "String::is_empty"
    )]
    description: String,
    #[serde(deserialize_with = "regular_expressions")]
    regular_expressions: Vec<String>,
    #[serde(default)]
    say_phrase: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    language_code: String,
    #[serde(default, skip_serializing_if = "option_string_is_empty")]
    function: Option<String>,
}

impl PhraseFilterItem {
    pub(crate) fn from_projection(
        name: String,
        description: String,
        regular_expressions: Vec<String>,
        say_phrase: bool,
        language_code: String,
        function: Option<String>,
    ) -> Result<Self, String> {
        Ok(Self {
            name: NonEmptyString::new(name)?,
            description: description.trim().to_string(),
            regular_expressions,
            say_phrase,
            language_code,
            function,
        })
    }

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

fn option_string_is_empty(value: &Option<String>) -> bool {
    value.as_deref().is_none_or(str::is_empty)
}
