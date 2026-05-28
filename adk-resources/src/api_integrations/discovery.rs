use crate::discover::DiscoverResources;
use crate::local_resources::{is_file, read_yaml_mapping, validate_named_sequence};
use crate::resource_utils::{clean_name, rel_under_root};
use serde_yaml::Value;
use std::path::Path;

// poly/resources/api_integration.py
pub(crate) struct ApiIntegration;
impl DiscoverResources for ApiIntegration {
    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let path = base_path.join("config/api_integrations.yaml");
        if !is_file(fs, &path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(fs, &path) else {
            return vec![];
        };
        let Some(Value::Sequence(list)) = m.get("api_integrations") else {
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
                &path.join("api_integrations").join(&safe),
            ));
        }
        out
    }
}

pub(crate) fn validate_local_yaml(yaml: &serde_yaml::Value, errors: &mut Vec<String>) {
    validate_named_sequence(
        "config/api_integrations.yaml",
        yaml,
        "api_integrations",
        "API integration",
        errors,
    );
    let Some(items) = yaml
        .get("api_integrations")
        .and_then(serde_yaml::Value::as_sequence)
    else {
        return;
    };
    for item in items {
        let Some(raw_name) = item.get("name").and_then(serde_yaml::Value::as_str) else {
            continue;
        };
        let name = clean_name(raw_name, false);
        if !is_python_function_name(&name) {
            errors.push(format!(
                "Validation error in config/api_integrations.yaml/api_integrations/{name}: API integration name '{name}' must follow Python function naming convention (lowercase letters, numbers, and underscores only, starting with letter or underscore)."
            ));
        }
    }
}

fn is_python_function_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_lowercase())
        && chars.all(|ch| ch == '_' || ch.is_ascii_lowercase() || ch.is_ascii_digit())
}
