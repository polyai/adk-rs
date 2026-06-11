use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::{clean_name, rel_under_root};
use serde_yaml_ng::Value;
use std::path::Path;

// poly/resources/keyphrase_boosting.py
/// Validation parity: implemented against Python KeyphraseBoosting.validate().
pub(crate) struct KeyphraseBoosting;
impl DiscoverResources for KeyphraseBoosting {
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::InFile {
        path: crate::specs::KEYPHRASE_BOOSTING_FILE.file_path,
        yaml_path: &["keyphrases"],
    };

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let yaml_path = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if !is_file(fs, &yaml_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(fs, &yaml_path) else {
            return vec![];
        };
        let Some(Value::Sequence(list)) = m.get("keyphrases") else {
            return vec![];
        };
        let mut out = Vec::new();
        for item in list {
            let Value::Mapping(map) = item else { continue };
            let Some(name) = map.get("keyphrase").and_then(|v| v.as_str()) else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            let safe = clean_name(name, false);
            out.push(rel_under_root(
                base_path,
                &yaml_path.join("keyphrases").join(&safe),
            ));
        }
        out
    }

    fn validate_local_yaml(_path: &str, yaml: &Value, errors: &mut Vec<String>) {
        validate_local_yaml(yaml, errors);
    }
}

pub(crate) fn validate_local_yaml(yaml: &Value, errors: &mut Vec<String>) {
    let path = KeyphraseBoosting::LOCAL_PATH
        .primary_path()
        .expect("local file path");
    let Some(keyphrases) = yaml.get("keyphrases").and_then(Value::as_sequence) else {
        return;
    };
    for (idx, keyphrase) in keyphrases.iter().enumerate() {
        let keyphrase_text = keyphrase
            .get("keyphrase")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if keyphrase_text.is_empty() {
            errors.push(format!(
                "Validation error in {path}/keyphrases/{idx}: Keyphrase is required"
            ));
        }
        let level = keyphrase
            .get("level")
            .and_then(Value::as_str)
            .unwrap_or("default")
            .to_lowercase();
        if !matches!(level.as_str(), "default" | "boosted" | "maximum") {
            errors.push(format!(
                "Validation error in {path}/keyphrases/{keyphrase_text}: Invalid level '{level}'. Must be one of: default, boosted, maximum"
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml_ng::from_str;

    fn validation_errors(yaml: &str) -> Vec<String> {
        let yaml = from_str::<Value>(yaml).expect("keyphrase YAML");
        let mut errors = Vec::new();
        validate_local_yaml(&yaml, &mut errors);
        errors
    }

    #[test]
    fn validates_python_keyphrase_required_and_level_rules() {
        let errors = validation_errors(
            r#"
keyphrases:
  - keyphrase: ""
    level: boosted
  - keyphrase: Open sesame
    level: loud
"#,
        );

        assert!(
            errors
                .iter()
                .any(|error| error.contains("Keyphrase is required"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("Invalid level 'loud'"))
        );
    }
}
