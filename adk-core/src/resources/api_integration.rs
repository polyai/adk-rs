use crate::discover::DiscoverResources;
use crate::discover::resource_utils::{clean_name, rel_under_root};
use crate::resources::common::{is_file, read_yaml_mapping};
use serde_yaml::Value;
use std::path::Path;

// poly/resources/api_integration.py
pub(crate) struct ApiIntegration;
impl DiscoverResources for ApiIntegration {
    const TYPE_NAME: &'static str = "ApiIntegration";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let path = base_path.join("config/api_integrations.yaml");
        if !is_file(&path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(&path) else {
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
