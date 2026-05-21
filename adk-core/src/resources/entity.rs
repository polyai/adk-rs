use crate::discover::DiscoverResources;
use crate::discover::resource_utils::{clean_name, rel_under_root};
use crate::resources::common::{is_file, read_yaml_mapping};
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
