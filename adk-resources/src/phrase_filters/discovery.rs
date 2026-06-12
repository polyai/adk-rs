use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_parse::{
    NonEmptyString, ParseLocalResource, ResourceParseErrors, deserialize_yaml, non_empty_vec,
};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::{clean_name, rel_under_root};
use serde::Deserialize;
use serde_yaml_ng::Value;
use std::path::Path;

// poly/resources/phrase_filter.py
/// Validation parity: TODO(DEVP-319) audit Python PhraseFilter.validate().
pub(crate) struct PhraseFilter;
impl DiscoverResources for PhraseFilter {
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::InFile {
        path: "voice/response_control/phrase_filtering.yaml",
        yaml_path: &["phrase_filters"],
    };

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let yaml_path = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if !is_file(fs, &yaml_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(fs, &yaml_path) else {
            return vec![];
        };
        let Some(Value::Sequence(list)) = m.get("phrase_filtering") else {
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
                &yaml_path.join("phrase_filtering").join(&safe),
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
    let path = PhraseFilter::LOCAL_PATH
        .primary_path()
        .expect("local file path");
    <PhraseFilter as ParseLocalResource>::validate_local_yaml(path, yaml, errors);
}

impl ParseLocalResource for PhraseFilter {
    type Parsed = PhraseFiltersFile;

    fn parse_local_yaml(path: &str, yaml: &Value) -> Result<Self::Parsed, ResourceParseErrors> {
        deserialize_yaml(path, yaml)
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(crate) struct PhraseFiltersFile {
    #[serde(default)]
    phrase_filtering: Vec<PhraseFilterItem>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct PhraseFilterItem {
    name: NonEmptyString,
    #[serde(deserialize_with = "regular_expressions")]
    regular_expressions: Vec<String>,
}

fn regular_expressions<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    non_empty_vec(deserializer, "At least one regular expression is required")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml_ng::from_str;

    fn validation_errors(yaml: &str) -> Vec<String> {
        let yaml = from_str::<Value>(yaml).expect("phrase filter YAML");
        let mut errors = Vec::new();
        validate_local_yaml(&yaml, &mut errors);
        errors
    }

    #[test]
    fn validates_python_phrase_filter_local_required_fields() {
        let missing_name = validation_errors(
            r#"
phrase_filtering:
  - description: Missing all
"#,
        );
        assert!(
            missing_name
                .iter()
                .any(|error| error.contains("missing field `name`"))
        );

        let empty_regex = validation_errors(
            r#"
phrase_filtering:
  - name: Empty regex
    regular_expressions: []
"#,
        );

        assert!(
            empty_regex
                .iter()
                .any(|error| error.contains("At least one regular expression is required"))
        );
    }
}
