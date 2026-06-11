use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::{clean_name, rel_under_root};
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
        validate_local_yaml(yaml, errors);
    }
}

pub(crate) fn validate_local_yaml(yaml: &Value, errors: &mut Vec<String>) {
    let path = PhraseFilter::LOCAL_PATH
        .primary_path()
        .expect("local file path");
    let Some(filters) = yaml.get("phrase_filtering").and_then(Value::as_sequence) else {
        return;
    };
    for (idx, filter) in filters.iter().enumerate() {
        let name = filter.get("name").and_then(Value::as_str).unwrap_or("");
        let error_path = if name.is_empty() {
            format!("{path}/phrase_filtering/{idx}")
        } else {
            format!("{path}/phrase_filtering/{}", clean_name(name, false))
        };
        if name.is_empty() {
            errors.push(format!(
                "Validation error in {error_path}: Name is required"
            ));
        }
        if filter
            .get("regular_expressions")
            .and_then(Value::as_sequence)
            .is_none_or(Vec::is_empty)
        {
            errors.push(format!(
                "Validation error in {error_path}: At least one regular expression is required"
            ));
        }
    }
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
        let errors = validation_errors(
            r#"
phrase_filtering:
  - description: Missing all
  - name: Empty regex
    regular_expressions: []
"#,
        );

        assert!(
            errors
                .iter()
                .any(|error| error.contains("Name is required"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("At least one regular expression is required"))
        );
    }
}
