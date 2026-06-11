use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_resources::{is_file, read_yaml_mapping, validate_named_sequence};
use crate::resource_utils::{clean_name, rel_under_root};
use serde_yaml_ng::Value;
use std::path::Path;

// poly/resources/sms.py
/// Validation parity: TODO(DEVP-319) audit Python SMSTemplate.validate().
pub(crate) struct SMSTemplate;
impl DiscoverResources for SMSTemplate {
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::InFile {
        path: "config/sms_templates.yaml",
        yaml_path: &["sms_templates"],
    };

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let path = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if !is_file(fs, &path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(fs, &path) else {
            return vec![];
        };
        let Some(Value::Sequence(list)) = m.get("sms_templates") else {
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
                &path.join("sms_templates").join(&safe),
            ));
        }
        out
    }

    fn validate_local_yaml(_path: &str, yaml: &Value, errors: &mut Vec<String>) {
        validate_local_yaml(yaml, errors);
    }
}

pub(crate) fn validate_local_yaml(yaml: &Value, errors: &mut Vec<String>) {
    let path = SMSTemplate::LOCAL_PATH
        .primary_path()
        .expect("local file path");
    validate_named_sequence(path, yaml, "sms_templates", "SMS template", errors);
    let Some(templates) = yaml.get("sms_templates").and_then(Value::as_sequence) else {
        return;
    };
    for (idx, template) in templates.iter().enumerate() {
        let name = template.get("name").and_then(Value::as_str).unwrap_or("");
        let error_path = if name.is_empty() {
            format!("{path}/sms_templates/{idx}")
        } else {
            format!("{path}/sms_templates/{}", clean_name(name, false))
        };
        if template
            .get("text")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            errors.push(format!(
                "Validation error in {error_path}: Text is required"
            ));
        }
        if template.get("env_phone_numbers").is_none() {
            errors.push(format!(
                "Validation error in {error_path}: Env phone numbers are required"
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml_ng::from_str;

    fn validation_errors(yaml: &str) -> Vec<String> {
        let yaml = from_str::<Value>(yaml).expect("SMS templates YAML");
        let mut errors = Vec::new();
        validate_local_yaml(&yaml, &mut errors);
        errors
    }

    #[test]
    fn validates_python_sms_template_local_required_fields() {
        let errors = validation_errors(
            r#"
sms_templates:
  - name: Empty text
    text: ""
  - text: Hello
"#,
        );

        assert!(
            errors
                .iter()
                .any(|error| error.contains("Text is required"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("Env phone numbers are required"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("SMS template name is required"))
        );
    }
}
