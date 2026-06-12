use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::handoffs::local::{HANDOFFS_FILE_PATH, HandoffsFile, parse_handoffs_file};
use crate::local_parse::{ParseLocalResource, ResourceParseResult};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::{clean_name, rel_under_root};
use serde_yaml_ng::Value;
use std::path::Path;

// poly/resources/handoff.py
/// Validation parity: implemented against Python Handoff.validate() and Handoff.validate_collection().
pub(crate) struct Handoff;
impl DiscoverResources for Handoff {
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::InFile {
        path: HANDOFFS_FILE_PATH,
        yaml_path: &["handoffs"],
    };

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let path = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if !is_file(fs, &path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(fs, &path) else {
            return vec![];
        };
        let Some(Value::Sequence(list)) = m.get("handoffs") else {
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
                &path.join("handoffs").join(&safe),
            ));
        }
        out
    }

    fn append_local_resource_errors(_path: &str, yaml: &Value, errors: &mut Vec<String>) {
        <Self as ParseLocalResource>::append_parse_errors(
            Self::LOCAL_PATH.primary_path().expect("local file path"),
            yaml,
            errors,
        );
    }
}

#[cfg(test)]
pub(crate) fn append_parse_errors(yaml: &Value, errors: &mut Vec<String>) {
    let path = Handoff::LOCAL_PATH.primary_path().expect("local file path");
    <Handoff as ParseLocalResource>::append_parse_errors(path, yaml, errors);
}

impl ParseLocalResource for Handoff {
    type Parsed = HandoffsFile;

    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed> {
        parse_handoffs_file(path, yaml)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml_ng::from_str;

    fn validation_errors(yaml: &str) -> Vec<String> {
        let yaml = from_str::<Value>(yaml).expect("handoffs YAML");
        let mut errors = Vec::new();
        append_parse_errors(&yaml, &mut errors);
        errors
    }

    #[test]
    fn validates_python_handoff_sip_rules_and_default_collection() {
        let invalid_method = validation_errors(
            r#"
handoffs:
  - name: Primary
    is_default: true
    sip_config:
      method: transfer
"#,
        );
        assert!(
            invalid_method
                .iter()
                .any(|error| error.contains("Invalid SIP method 'transfer'"))
        );

        let invalid_encryption = validation_errors(
            r#"
handoffs:
  - name: Backup
    is_default: true
    sip_config:
      method: invite
      outbound_encryption: plaintext
"#,
        );
        assert!(
            invalid_encryption
                .iter()
                .any(|error| error.contains("Invalid encryption method 'plaintext'"))
        );

        let missing_default = validation_errors(
            r#"
handoffs:
  - name: Primary
    is_default: false
"#,
        );
        assert!(
            missing_default
                .iter()
                .any(|error| error.contains("Multiple or zero default handoffs detected: []"))
        );
    }
}
