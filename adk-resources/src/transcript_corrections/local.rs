use crate::local_parse::{
    NonEmptyString, ResourceParseErrors, ResourceParseResult, deserialize_yaml, duplicate_names,
    non_empty_vec,
};
use crate::resource_utils::clean_name;
use serde::{Deserialize, Serialize};
use serde_yaml_ng::Value;

#[derive(Debug, Serialize)]
pub(crate) struct TranscriptCorrectionsFile {
    pub(crate) corrections: Vec<TranscriptCorrectionItem>,
}

impl TranscriptCorrectionsFile {
    pub(crate) fn new(corrections: Vec<TranscriptCorrectionItem>) -> Self {
        Self { corrections }
    }

    fn try_from_raw(path: &str, raw: RawTranscriptCorrectionsFile) -> ResourceParseResult<Self> {
        let mut errors = ResourceParseErrors::new();
        for duplicate in duplicate_names(raw.corrections.iter().map(|item| item.name.as_str())) {
            errors.push(
                path,
                format!("Duplicate transcript correction names: ['{duplicate}']"),
            );
        }
        if errors.is_empty() {
            Ok(Self {
                corrections: raw.corrections,
            })
        } else {
            Err(errors)
        }
    }
}

pub(crate) fn parse_transcript_corrections_file(
    path: &str,
    yaml: &Value,
) -> ResourceParseResult<TranscriptCorrectionsFile> {
    let mut errors = transcript_correction_item_errors(path, yaml);
    match deserialize_yaml::<RawTranscriptCorrectionsFile>(path, yaml) {
        Ok(raw) if errors.is_empty() => TranscriptCorrectionsFile::try_from_raw(path, raw),
        Ok(raw) => {
            if let Err(parse_errors) = TranscriptCorrectionsFile::try_from_raw(path, raw) {
                errors.extend(parse_errors);
            }
            Err(errors)
        }
        Err(parse_errors) => {
            if !errors.is_empty()
                && parse_errors
                    .clone()
                    .into_validation_errors()
                    .iter()
                    .all(|error| error.contains("At least one regular expression rule is required"))
            {
                Err(errors)
            } else {
                errors.extend(parse_errors);
                Err(errors)
            }
        }
    }
}

pub(crate) fn parse_transcript_corrections_content(
    path: &str,
    content: &str,
) -> ResourceParseResult<TranscriptCorrectionsFile> {
    let yaml = serde_yaml_ng::from_str::<Value>(content)
        .map_err(|error| ResourceParseErrors::single(path, error))?;
    parse_transcript_corrections_file(path, &yaml)
}

#[derive(Debug, Deserialize)]
struct RawTranscriptCorrectionsFile {
    #[serde(default)]
    corrections: Vec<TranscriptCorrectionItem>,
}

#[derive(Debug, Deserialize)]
struct TranscriptCorrectionItemsUnchecked {
    #[serde(default)]
    corrections: Vec<TranscriptCorrectionItemUnchecked>,
}

#[derive(Debug, Deserialize)]
struct TranscriptCorrectionItemUnchecked {
    name: Option<String>,
    regular_expressions: Option<Vec<Value>>,
}

fn transcript_correction_item_errors(path: &str, yaml: &Value) -> ResourceParseErrors {
    let mut errors = ResourceParseErrors::new();
    let Ok(raw) = deserialize_yaml::<TranscriptCorrectionItemsUnchecked>(path, yaml) else {
        return errors;
    };
    for item in raw.corrections {
        if item.regular_expressions.as_ref().is_some_and(Vec::is_empty) {
            let item_path = item
                .name
                .filter(|name| !name.is_empty())
                .map(|name| format!("{path}/corrections/{}", clean_name(&name, false)))
                .unwrap_or_else(|| path.to_string());
            errors.push(
                &item_path,
                "At least one regular expression rule is required",
            );
        }
    }
    errors
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct TranscriptCorrectionItem {
    name: NonEmptyString,
    #[serde(
        default,
        deserialize_with = "deserialize_trimmed_string",
        skip_serializing_if = "String::is_empty"
    )]
    description: String,
    #[serde(deserialize_with = "regular_expression_rules")]
    regular_expressions: Vec<RegularExpressionRule>,
}

impl TranscriptCorrectionItem {
    pub(crate) fn from_projection(
        name: String,
        description: String,
        regular_expressions: Vec<RegularExpressionRule>,
    ) -> Result<Self, String> {
        Ok(Self {
            name: NonEmptyString::new(name)?,
            description: description.trim().to_string(),
            regular_expressions,
        })
    }

    pub(crate) fn name(&self) -> &str {
        self.name.as_str()
    }

    pub(crate) fn description(&self) -> &str {
        &self.description
    }

    pub(crate) fn regular_expressions(&self) -> &[RegularExpressionRule] {
        &self.regular_expressions
    }
}

fn regular_expression_rules<'de, D>(deserializer: D) -> Result<Vec<RegularExpressionRule>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    non_empty_vec(
        deserializer,
        "At least one regular expression rule is required",
    )
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct RegularExpressionRule {
    regular_expression: NonEmptyString,
    #[serde(default)]
    replacement: String,
    #[serde(default)]
    replacement_type: ReplacementType,
}

impl RegularExpressionRule {
    pub(crate) fn new(
        regular_expression: String,
        replacement: String,
        replacement_type: String,
    ) -> Result<Self, String> {
        Ok(Self {
            regular_expression: NonEmptyString::new(regular_expression)?,
            replacement,
            replacement_type: ReplacementType::from_backend_value(&replacement_type),
        })
    }

    pub(crate) fn regular_expression(&self) -> &str {
        self.regular_expression.as_str()
    }

    pub(crate) fn replacement(&self) -> &str {
        &self.replacement
    }

    pub(crate) fn backend_replacement_type(&self) -> &'static str {
        self.replacement_type.backend_str()
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum ReplacementType {
    #[default]
    Full,
    Partial,
    Substring,
}

impl ReplacementType {
    fn from_backend_value(value: &str) -> Self {
        match value {
            "partial" => Self::Partial,
            "substring" => Self::Substring,
            _ => Self::Full,
        }
    }

    fn backend_str(&self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Partial | Self::Substring => "partial",
        }
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
