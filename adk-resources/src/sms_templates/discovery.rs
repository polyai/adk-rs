use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_parse::{
    NonEmptyString, ParseLocalResource, ResourceParseErrors, ResourceParseResult, deserialize_yaml,
    duplicate_names,
};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::{clean_name, rel_under_root};
use serde::Deserialize;
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
        <Self as ParseLocalResource>::validate_local_yaml(
            Self::LOCAL_PATH.primary_path().expect("local file path"),
            yaml,
            errors,
        );
    }
}

#[cfg(test)]
pub(crate) fn validate_local_yaml(yaml: &Value, errors: &mut Vec<String>) {
    let path = SMSTemplate::LOCAL_PATH
        .primary_path()
        .expect("local file path");
    <SMSTemplate as ParseLocalResource>::validate_local_yaml(path, yaml, errors);
}

impl ParseLocalResource for SMSTemplate {
    type Parsed = SMSTemplatesFile;

    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed> {
        let raw = deserialize_yaml::<SMSTemplatesFileUnchecked>(path, yaml)?;
        SMSTemplatesFile::try_from_unchecked(path, raw)
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct SMSTemplatesFile {
    sms_templates: Vec<SMSTemplateItem>,
}

impl SMSTemplatesFile {
    fn try_from_unchecked(path: &str, raw: SMSTemplatesFileUnchecked) -> ResourceParseResult<Self> {
        let mut errors = ResourceParseErrors::new();
        for duplicate in duplicate_names(raw.sms_templates.iter().map(|item| item.name.as_str())) {
            errors.push(
                &format!("{path}/sms_templates/{duplicate}"),
                format!("duplicate SMS template name '{duplicate}'."),
            );
        }
        if errors.is_empty() {
            Ok(Self {
                sms_templates: raw.sms_templates,
            })
        } else {
            Err(errors)
        }
    }
}

#[derive(Debug, Deserialize)]
struct SMSTemplatesFileUnchecked {
    #[serde(default)]
    sms_templates: Vec<SMSTemplateItem>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SMSTemplateItem {
    name: NonEmptyString,
    text: NonEmptyString,
    env_phone_numbers: EnvPhoneNumbers,
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
struct EnvPhoneNumbers {
    #[serde(default)]
    sandbox: String,
    #[serde(default, alias = "preRelease")]
    pre_release: String,
    #[serde(default)]
    live: String,
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
