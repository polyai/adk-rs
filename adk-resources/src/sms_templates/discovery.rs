use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_parse::{ParseLocalResource, ResourceParseResult};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::{clean_name, rel_under_root};
use crate::sms_templates::local::{
    SMS_TEMPLATES_FILE_PATH, SmsTemplatesFile, parse_sms_templates_file,
};
use serde_yaml_ng::Value;
use std::path::Path;

// poly/resources/sms.py
/// Validation parity: TODO(DEVP-319) audit Python SMSTemplate.validate().
pub(crate) struct SMSTemplate;
impl DiscoverResources for SMSTemplate {
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::InFile {
        path: SMS_TEMPLATES_FILE_PATH,
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
    let path = SMSTemplate::LOCAL_PATH
        .primary_path()
        .expect("local file path");
    <SMSTemplate as ParseLocalResource>::append_parse_errors(path, yaml, errors);
}

impl ParseLocalResource for SMSTemplate {
    type Parsed = SmsTemplatesFile;

    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed> {
        parse_sms_templates_file(path, yaml)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml_ng::from_str;

    fn validation_errors(yaml: &str) -> Vec<String> {
        let yaml = from_str::<Value>(yaml).expect("SMS templates YAML");
        let mut errors = Vec::new();
        append_parse_errors(&yaml, &mut errors);
        errors
    }

    #[test]
    fn validates_python_sms_template_local_required_fields() {
        let empty_text = validation_errors(
            r#"
sms_templates:
  - name: Empty text
    text: ""
    env_phone_numbers: {}
"#,
        );

        assert!(
            empty_text
                .iter()
                .any(|error| error.contains("cannot be empty"))
        );

        let missing_env_phone_numbers = validation_errors(
            r#"
sms_templates:
  - name: Missing env
    text: Hello
"#,
        );
        assert!(
            missing_env_phone_numbers
                .iter()
                .any(|error| error.contains("missing field `env_phone_numbers`"))
        );

        let missing_name = validation_errors(
            r#"
sms_templates:
  - text: Hello
    env_phone_numbers: {}
"#,
        );
        assert!(
            missing_name
                .iter()
                .any(|error| error.contains("missing field `name`"))
        );
    }
}
