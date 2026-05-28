use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_resources::{is_file, read_yaml_mapping, validate_named_sequence};
use crate::resource_utils::{clean_name, rel_under_root};
use serde_yaml::Value;
use std::path::Path;

// poly/resources/sms.py
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

    fn validate_local_yaml(_path: &str, yaml: &serde_yaml::Value, errors: &mut Vec<String>) {
        validate_local_yaml(yaml, errors);
    }
}

pub(crate) fn validate_local_yaml(yaml: &serde_yaml::Value, errors: &mut Vec<String>) {
    let path = SMSTemplate::LOCAL_PATH
        .primary_path()
        .expect("local file path");
    validate_named_sequence(path, yaml, "sms_templates", "SMS template", errors);
}
