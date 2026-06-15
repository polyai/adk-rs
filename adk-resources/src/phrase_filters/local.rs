use crate::local_parse::{
    NonEmptyString, ResourceParseErrors, ResourceParseResult, deserialize_yaml, non_empty_vec,
};
use serde::{Deserialize, Serialize};
use serde_yaml_ng::Value;

pub(crate) const PHRASE_FILTERS_FILE_PATH: &str = "voice/response_control/phrase_filtering.yaml";
pub(crate) const PHRASE_FILTER_ITEM_PREFIX: &str =
    "voice/response_control/phrase_filtering.yaml/phrase_filtering/";

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

pub(crate) fn deserialize_phrase_filters_content(
    path: &str,
    content: &str,
) -> ResourceParseResult<Vec<PhraseFilterItem>> {
    let yaml = serde_yaml_ng::from_str::<Value>(content)
        .map_err(|error| ResourceParseErrors::single(path, error))?;
    if path == PHRASE_FILTERS_FILE_PATH {
        return deserialize_yaml::<LenientPhraseFiltersFile>(path, &yaml)
            .map(|file| file.phrase_filtering.into_iter().map(Into::into).collect());
    }
    if path.starts_with(PHRASE_FILTER_ITEM_PREFIX) {
        return deserialize_yaml::<LenientPhraseFilterItem>(path, &yaml)
            .map(|item| vec![item.into()]);
    }
    Ok(Vec::new())
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

#[derive(Debug, Deserialize)]
struct LenientPhraseFiltersFile {
    #[serde(default)]
    phrase_filtering: Vec<LenientPhraseFilterItem>,
}

#[derive(Debug, Deserialize)]
struct LenientPhraseFilterItem {
    name: NonEmptyString,
    #[serde(default, deserialize_with = "deserialize_trimmed_string")]
    description: String,
    #[serde(default)]
    regular_expressions: Vec<String>,
    #[serde(default)]
    say_phrase: bool,
    #[serde(default)]
    language_code: String,
    #[serde(default)]
    function: Option<String>,
}

impl From<LenientPhraseFilterItem> for PhraseFilterItem {
    fn from(item: LenientPhraseFilterItem) -> Self {
        Self {
            name: item.name,
            description: item.description,
            regular_expressions: item.regular_expressions,
            say_phrase: item.say_phrase,
            language_code: item.language_code,
            function: item.function,
        }
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
