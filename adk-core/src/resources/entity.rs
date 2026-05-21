use crate::discover::DiscoverResources;
use crate::discover::resource_utils::{clean_name, rel_under_root};
use crate::resources::common::{is_file, read_yaml_mapping, validate_named_sequence};
use serde_yaml::Value;
use std::path::Path;

// poly/resources/entities.py
pub(crate) struct Entity;
impl DiscoverResources for Entity {
    const TYPE_NAME: &'static str = "Entity";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let entities_path = base_path.join("config/entities.yaml");
        if !is_file(&entities_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(&entities_path) else {
            return vec![];
        };
        let Some(Value::Sequence(list)) = m.get("entities") else {
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
                &entities_path.join("entities").join(&safe),
            ));
        }
        out
    }
}

pub(crate) fn validate_local_yaml(yaml: &serde_yaml::Value, errors: &mut Vec<String>) {
    validate_named_sequence("config/entities.yaml", yaml, "entities", "entity", errors);
    let Some(items) = yaml
        .get("entities")
        .and_then(serde_yaml::Value::as_sequence)
    else {
        return;
    };
    let allowed = [
        "numeric",
        "alphanumeric",
        "enum",
        "date",
        "phone_number",
        "time",
        "address",
        "free_text",
        "name_config",
    ];
    for item in items {
        let name = item
            .get("name")
            .and_then(serde_yaml::Value::as_str)
            .unwrap_or("<missing>");
        let Some(entity_type) = item.get("entity_type").and_then(serde_yaml::Value::as_str) else {
            errors.push(format!(
                "Validation error in config/entities.yaml/entities/{name}: entity_type is required."
            ));
            continue;
        };
        if !allowed.contains(&entity_type) {
            errors.push(format!(
                "Validation error in config/entities.yaml/entities/{name}: unsupported entity_type '{entity_type}'."
            ));
        }
    }
}
