use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_parse::{
    NonEmptyString, ParseLocalResource, ResourceParseErrors, ResourceParseResult, deserialize_yaml,
    duplicate_names,
};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::{clean_name, rel_under_root};
use serde::Deserialize;
use serde::de::{Error as DeError, Visitor};
use serde_yaml_ng::Value;
use std::fmt;
use std::path::Path;

// poly/resources/transcript_correction.py
/// Validation parity: implemented against Python TranscriptCorrection.validate() and TranscriptCorrection.validate_collection().
pub(crate) struct TranscriptCorrection;
impl DiscoverResources for TranscriptCorrection {
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::InFile {
        path: crate::specs::TRANSCRIPT_CORRECTIONS_FILE.file_path,
        yaml_path: &["corrections"],
    };

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let yaml_path = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if !is_file(fs, &yaml_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(fs, &yaml_path) else {
            return vec![];
        };
        let Some(Value::Sequence(list)) = m.get("corrections") else {
            return vec![];
        };
        let mut out = Vec::new();
        for item in list {
            let Value::Mapping(map) = item else { continue };
            let Some(name) = map.get("name").and_then(|v| v.as_str()) else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            let safe = clean_name(name, false);
            out.push(rel_under_root(
                base_path,
                &yaml_path.join("corrections").join(&safe),
            ));
        }
        out
    }

    fn validate_local_yaml(_path: &str, yaml: &Value, errors: &mut Vec<String>) {
        <Self as ParseLocalResource>::validate_local_yaml(
            Self::LOCAL_PATH.primary_path().expect("local file path"),
            yaml,
            errors,
        );
    }
}

#[cfg(test)]
pub(crate) fn validate_local_yaml(yaml: &Value, errors: &mut Vec<String>) {
    let path = TranscriptCorrection::LOCAL_PATH
        .primary_path()
        .expect("local file path");
    <TranscriptCorrection as ParseLocalResource>::validate_local_yaml(path, yaml, errors);
}

impl ParseLocalResource for TranscriptCorrection {
    type Parsed = TranscriptCorrectionsFile;

    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed> {
        let raw = deserialize_yaml::<TranscriptCorrectionsFileUnchecked>(path, yaml)?;
        TranscriptCorrectionsFile::try_from_unchecked(path, raw)
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct TranscriptCorrectionsFile {
    corrections: Vec<TranscriptCorrectionItem>,
}

impl TranscriptCorrectionsFile {
    fn try_from_unchecked(
        path: &str,
        raw: TranscriptCorrectionsFileUnchecked,
    ) -> ResourceParseResult<Self> {
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

#[derive(Debug, Deserialize)]
struct TranscriptCorrectionsFileUnchecked {
    #[serde(default)]
    corrections: Vec<TranscriptCorrectionItem>,
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "TranscriptCorrectionItemUnchecked")]
#[allow(dead_code)]
struct TranscriptCorrectionItem {
    name: NonEmptyString,
    regular_expressions: Vec<RegularExpressionRule>,
}

#[derive(Debug, Deserialize)]
struct TranscriptCorrectionItemUnchecked {
    name: NonEmptyString,
    #[serde(default)]
    regular_expressions: Vec<RegularExpressionRule>,
}

impl TryFrom<TranscriptCorrectionItemUnchecked> for TranscriptCorrectionItem {
    type Error = String;

    fn try_from(raw: TranscriptCorrectionItemUnchecked) -> Result<Self, Self::Error> {
        if raw.regular_expressions.is_empty() {
            return Err("At least one regular expression rule is required".to_string());
        }
        Ok(Self {
            name: raw.name,
            regular_expressions: raw.regular_expressions,
        })
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RegularExpressionRule {
    regular_expression: NonEmptyString,
    #[serde(default)]
    replacement_type: ReplacementType,
}

#[derive(Debug, Default)]
enum ReplacementType {
    #[default]
    Full,
    Partial,
    Substring,
}

impl<'de> Deserialize<'de> for ReplacementType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ReplacementTypeVisitor;

        impl Visitor<'_> for ReplacementTypeVisitor {
            type Value = ReplacementType;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("full, partial, or substring")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                match value {
                    "full" => Ok(ReplacementType::Full),
                    "partial" => Ok(ReplacementType::Partial),
                    "substring" => Ok(ReplacementType::Substring),
                    _ => Err(E::custom(format!(
                        "Invalid replacement_type '{value}'. Must be one of: full, partial, substring"
                    ))),
                }
            }
        }

        deserializer.deserialize_str(ReplacementTypeVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml_ng::from_str;

    fn validation_errors(yaml: &str) -> Vec<String> {
        let yaml = from_str::<Value>(yaml).expect("transcript correction YAML");
        let mut errors = Vec::new();
        validate_local_yaml(&yaml, &mut errors);
        errors
    }

    #[test]
    fn validates_python_transcript_correction_rule_fields_and_duplicates() {
        let missing_regex = validation_errors(
            r#"
corrections:
  - name: Fix alpha
    regular_expressions:
      - regular_expression: ""
"#,
        );
        assert!(
            missing_regex
                .iter()
                .any(|error| error.contains("cannot be empty"))
        );

        let invalid_replacement_type = validation_errors(
            r#"
corrections:
  - name: Fix alpha
    regular_expressions:
      - regular_expression: abc
        replacement_type: typo
"#,
        );
        assert!(
            invalid_replacement_type
                .iter()
                .any(|error| error.contains("Invalid replacement_type 'typo'"))
        );

        let empty_rules = validation_errors(
            r#"
corrections:
  - name: Fix alpha
    regular_expressions: []
"#,
        );
        assert!(
            empty_rules
                .iter()
                .any(|error| error.contains("At least one regular expression rule is required"))
        );

        let duplicates = validation_errors(
            r#"
corrections:
  - name: Fix alpha
    regular_expressions:
      - regular_expression: abc
  - name: Fix alpha
    regular_expressions:
      - regular_expression: def
"#,
        );
        assert!(
            duplicates
                .iter()
                .any(|error| error.contains("Duplicate transcript correction names: ['Fix alpha']"))
        );

        let missing_name = validation_errors(
            r#"
corrections:
  - description: Missing name
    regular_expressions:
      - regular_expression: abc
        replacement: def
"#,
        );
        assert!(
            missing_name
                .iter()
                .any(|error| error.contains("missing field `name`"))
        );
    }
}
