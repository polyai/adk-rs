use crate::discover::DiscoverResources;
use crate::discover::resource_utils::{clean_name, rel_under_root};
use crate::resources::common::{is_file, read_yaml_mapping};
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
