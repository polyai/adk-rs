use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_resources::{is_file, read_yaml_mapping, validate_named_sequence};
use crate::resource_utils::{clean_name, rel_under_root};
use serde_yaml_ng::Value;
use std::path::Path;

// poly/resources/entities.py
pub(crate) struct Entity;
impl DiscoverResources for Entity {
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::InFile {
        path: crate::specs::ENTITIES_FILE.file_path,
        yaml_path: &["entities"],
    };

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let entities_path =
            base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if !is_file(fs, &entities_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(fs, &entities_path) else {
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

    fn validate_local_yaml(_path: &str, yaml: &Value, errors: &mut Vec<String>) {
        validate_local_yaml(yaml, errors);
    }
}

pub(crate) fn validate_local_yaml(yaml: &Value, errors: &mut Vec<String>) {
    let path = Entity::LOCAL_PATH.primary_path().expect("local file path");
    validate_named_sequence(path, yaml, "entities", "entity", errors);
    let Some(items) = yaml.get("entities").and_then(Value::as_sequence) else {
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
            .and_then(Value::as_str)
            .unwrap_or("<missing>");
        let Some(entity_type) = item.get("entity_type").and_then(Value::as_str) else {
            errors.push(format!(
                "Validation error in {path}/entities/{name}: entity_type is required."
            ));
            continue;
        };
        if !allowed.contains(&entity_type) {
            errors.push(format!(
                "Validation error in {path}/entities/{name}: unsupported entity_type '{entity_type}'."
            ));
        }
    }
}
