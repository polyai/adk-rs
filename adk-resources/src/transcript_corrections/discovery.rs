use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::{clean_name, rel_under_root};
use serde_yaml_ng::Value;
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
        validate_local_yaml(yaml, errors);
    }
}

pub(crate) fn validate_local_yaml(yaml: &Value, errors: &mut Vec<String>) {
    let path = TranscriptCorrection::LOCAL_PATH
        .primary_path()
        .expect("local file path");
    let Some(corrections) = yaml.get("corrections").and_then(Value::as_sequence) else {
        return;
    };
    let mut seen_names = std::collections::BTreeSet::new();
    let mut duplicate_names = std::collections::BTreeSet::new();
    for (correction_idx, correction) in corrections.iter().enumerate() {
        let Some(raw_name) = correction.get("name").and_then(Value::as_str) else {
            errors.push(format!(
                "Validation error in {path}/corrections/{correction_idx}: Correction name is required"
            ));
            continue;
        };
        if raw_name.is_empty() {
            errors.push(format!(
                "Validation error in {path}/corrections/{correction_idx}: Correction name is required"
            ));
            continue;
        }
        if !seen_names.insert(raw_name.to_string()) {
            duplicate_names.insert(raw_name.to_string());
        }
        let name = clean_name(raw_name, false);
        let Some(rules) = correction
            .get("regular_expressions")
            .and_then(Value::as_sequence)
        else {
            errors.push(format!(
                "Validation error in {path}/corrections/{name}: At least one regular expression rule is required"
            ));
            continue;
        };
        if rules.is_empty() {
            errors.push(format!(
                "Validation error in {path}/corrections/{name}: At least one regular expression rule is required"
            ));
            continue;
        }
        for (idx, rule) in rules.iter().enumerate() {
            if rule
                .get("regular_expression")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
            {
                errors.push(format!(
                    "Validation error in {path}/corrections/{name}/regular_expressions/{idx}: Regular expression is required for rule {idx}"
                ));
            }
            let replacement_type = rule
                .get("replacement_type")
                .and_then(Value::as_str)
                .unwrap_or("full");
            if !matches!(replacement_type, "full" | "partial" | "substring") {
                errors.push(format!(
                    "Validation error in {path}/corrections/{name}/regular_expressions/{idx}: Invalid replacement_type '{replacement_type}' in rule {idx}. Must be one of: full, partial, substring"
                ));
            }
        }
    }
    if !duplicate_names.is_empty() {
        errors.push(format!(
            "Validation error: Duplicate transcript correction names: [{}]",
            duplicate_names
                .iter()
                .map(|name| format!("'{name}'"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
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
        let errors = validation_errors(
            r#"
corrections:
  - name: Fix alpha
    regular_expressions:
      - regular_expression: ""
        replacement_type: typo
  - name: Fix alpha
    regular_expressions: []
  - description: Missing name
    regular_expressions:
      - regular_expression: abc
        replacement: def
"#,
        );

        assert!(
            errors
                .iter()
                .any(|error| error.contains("Regular expression is required for rule 0"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("Invalid replacement_type 'typo'"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("At least one regular expression rule is required"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("Duplicate transcript correction names: ['Fix alpha']"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("Correction name is required"))
        );
    }
}
