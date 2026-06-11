use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::{clean_name, rel_under_root};
use serde_yaml_ng::Value;
use std::path::Path;

// poly/resources/pronunciation.py
/// Validation parity: implemented against Python Pronunciation.validate().
pub(crate) struct Pronunciation;
impl DiscoverResources for Pronunciation {
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::InFile {
        path: crate::specs::PRONUNCIATIONS_FILE.file_path,
        yaml_path: &["pronunciations"],
    };

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let yaml_path = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if !is_file(fs, &yaml_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(fs, &yaml_path) else {
            return vec![];
        };
        let Some(Value::Sequence(items)) = m.get("pronunciations") else {
            return vec![];
        };
        let mut out = Vec::new();
        for (i, _item) in items.iter().enumerate() {
            let safe = clean_name(&i.to_string(), false);
            out.push(rel_under_root(
                base_path,
                &yaml_path.join("pronunciations").join(&safe),
            ));
        }
        out
    }

    fn validate_local_yaml(_path: &str, yaml: &Value, errors: &mut Vec<String>) {
        validate_local_yaml(yaml, errors);
    }
}

pub(crate) fn validate_local_yaml(yaml: &Value, errors: &mut Vec<String>) {
    let path = Pronunciation::LOCAL_PATH
        .primary_path()
        .expect("local file path");
    let Some(pronunciations) = yaml.get("pronunciations").and_then(Value::as_sequence) else {
        return;
    };
    for (idx, pronunciation) in pronunciations.iter().enumerate() {
        if pronunciation
            .get("regex")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            errors.push(format!(
                "Validation error in {path}/pronunciations/{idx}: Regex pattern is required"
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml_ng::from_str;

    #[test]
    fn validates_python_pronunciation_regex_required_rule() {
        let yaml = from_str::<Value>(
            r#"
pronunciations:
  - regex: ""
    replacement: poly
"#,
        )
        .expect("pronunciation YAML");
        let mut errors = Vec::new();

        validate_local_yaml(&yaml, &mut errors);

        assert!(
            errors
                .iter()
                .any(|error| error.contains("Regex pattern is required"))
        );
    }
}
