use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_resources::{is_file, read_yaml_mapping, validate_named_sequence};
use crate::resource_utils::{clean_name, rel_under_root};
use serde_yaml_ng::Value;
use std::path::Path;

// poly/resources/handoff.py
/// Validation parity: implemented against Python Handoff.validate() and Handoff.validate_collection().
pub(crate) struct Handoff;
impl DiscoverResources for Handoff {
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::InFile {
        path: "config/handoffs.yaml",
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

    fn validate_local_yaml(_path: &str, yaml: &Value, errors: &mut Vec<String>) {
        validate_local_yaml(yaml, errors);
    }
}

pub(crate) fn validate_local_yaml(yaml: &Value, errors: &mut Vec<String>) {
    let path = Handoff::LOCAL_PATH.primary_path().expect("local file path");
    validate_named_sequence(path, yaml, "handoffs", "handoff", errors);
    let Some(handoffs) = yaml.get("handoffs").and_then(Value::as_sequence) else {
        return;
    };
    let mut default_names = Vec::new();
    for handoff in handoffs {
        let name = handoff
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if handoff
            .get("is_default")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            default_names.push(name.to_string());
        }
        let Some(sip_config) = handoff.get("sip_config") else {
            continue;
        };
        let method = sip_config
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("bye");
        if !matches!(method, "invite" | "refer" | "bye") {
            errors.push(format!(
                "Validation error in {path}/handoffs/{name}: Invalid SIP method '{method}'. Must be one of: invite, refer, bye"
            ));
            continue;
        }
        if method == "invite" {
            let encryption = sip_config
                .get("outbound_encryption")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !matches!(encryption, "TLS/SRTP" | "UDP/RTP") {
                errors.push(format!(
                    "Validation error in {path}/handoffs/{name}: Invalid encryption method '{encryption}'. Must be one of: TLS/SRTP, UDP/RTP"
                ));
            }
        }
    }
    if default_names.len() != 1 {
        errors.push(format!(
            "Validation error: Multiple or zero default handoffs detected: [{}]. One handoff must be set as default.",
            python_string_list(&default_names)
        ));
    }
}

fn python_string_list(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("'{value}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml_ng::from_str;

    fn validation_errors(yaml: &str) -> Vec<String> {
        let yaml = from_str::<Value>(yaml).expect("handoffs YAML");
        let mut errors = Vec::new();
        validate_local_yaml(&yaml, &mut errors);
        errors
    }

    #[test]
    fn validates_python_handoff_sip_rules_and_default_collection() {
        let errors = validation_errors(
            r#"
handoffs:
  - name: Primary
    is_default: false
    sip_config:
      method: transfer
  - name: Backup
    is_default: false
    sip_config:
      method: invite
      outbound_encryption: plaintext
"#,
        );

        assert!(
            errors
                .iter()
                .any(|error| error.contains("Invalid SIP method 'transfer'"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("Invalid encryption method 'plaintext'"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("Multiple or zero default handoffs detected: []"))
        );
    }
}
