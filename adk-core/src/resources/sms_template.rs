use crate::discover::DiscoverResources;
use crate::discover::resource_utils::{clean_name, rel_under_root};
use crate::resources::common::{is_file, read_yaml_mapping, validate_named_sequence};
use serde_yaml::Value;
use std::path::Path;

// poly/resources/sms.py
pub(crate) struct SMSTemplate;
impl DiscoverResources for SMSTemplate {
    const TYPE_NAME: &'static str = "SMSTemplate";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let path = base_path.join("config/sms_templates.yaml");
        if !is_file(&path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(&path) else {
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
}

pub(crate) fn validate_local_yaml(yaml: &serde_yaml::Value, errors: &mut Vec<String>) {
    validate_named_sequence(
        "config/sms_templates.yaml",
        yaml,
        "sms_templates",
        "SMS template",
        errors,
    );
}
