use crate::discover::DiscoverResources;
use crate::discover::resource_utils::{clean_name, rel_under_root};
use crate::resources::common::{is_file, read_yaml_mapping, validate_duplicate_names};
use serde_yaml::Value;
use std::path::Path;

// poly/resources/variant_attributes.py
pub(crate) struct Variant;
impl DiscoverResources for Variant {
    const TYPE_NAME: &'static str = "Variant";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let path = base_path.join("config/variant_attributes.yaml");
        if !is_file(&path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(&path) else {
            return vec![];
        };
        let Some(Value::Sequence(list)) = m.get("variants") else {
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
                &path.join("variants").join(&safe),
            ));
        }
        out
    }
}

pub(crate) fn validate_local_yaml(yaml: &serde_yaml::Value, errors: &mut Vec<String>) {
    let Some(variants) = yaml
        .get("variants")
        .and_then(serde_yaml::Value::as_sequence)
    else {
        return;
    };
    validate_duplicate_names(
        "config/variant_attributes.yaml",
        "variants",
        "variant",
        variants,
        errors,
    );
    let default_names = variants
        .iter()
        .filter(|variant| {
            variant
                .get("is_default")
                .and_then(serde_yaml::Value::as_bool)
                .unwrap_or(false)
        })
        .filter_map(|variant| variant.get("name").and_then(serde_yaml::Value::as_str))
        .collect::<Vec<_>>();
    if default_names.len() != 1 {
        let names = default_names
            .iter()
            .map(|name| format!("'{name}'"))
            .collect::<Vec<_>>()
            .join(", ");
        errors.push(format!(
            "Validation error: Multiple or zero default variants detected: [{names}]. One variant must be set as default."
        ));
    }
}

pub(crate) struct VariantAttribute;
impl DiscoverResources for VariantAttribute {
    const TYPE_NAME: &'static str = "VariantAttribute";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let path = base_path.join("config/variant_attributes.yaml");
        if !is_file(&path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(&path) else {
            return vec![];
        };
        let Some(Value::Sequence(list)) = m.get("attributes") else {
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
                &path.join("attributes").join(&safe),
            ));
        }
        out
    }
}
