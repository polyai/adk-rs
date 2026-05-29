use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::{clean_name, rel_under_root};
use serde_yaml_ng::Value;
use std::path::Path;

// poly/resources/transcript_correction.py
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
    for correction in corrections {
        let Some(raw_name) = correction.get("name").and_then(Value::as_str) else {
            continue;
        };
        let regular_expression_count = correction
            .get("regular_expressions")
            .and_then(Value::as_sequence)
            .map(Vec::len)
            .unwrap_or(0);
        if regular_expression_count == 0 {
            let name = clean_name(raw_name, false);
            errors.push(format!(
                "Validation error in {path}/corrections/{name}: At least one regular expression rule is required"
            ));
        }
    }
}
