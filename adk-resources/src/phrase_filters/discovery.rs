use crate::discover::DiscoverResources;
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::{clean_name, rel_under_root};
use serde_yaml::Value;
use std::path::Path;

// poly/resources/phrase_filter.py
pub(crate) struct PhraseFilter;
impl DiscoverResources for PhraseFilter {
    fn discover_resources(base_path: &Path) -> Vec<String> {
        let yaml_path = base_path.join("voice/response_control/phrase_filtering.yaml");
        if !is_file(&yaml_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(&yaml_path) else {
            return vec![];
        };
        let Some(Value::Sequence(list)) = m.get("phrase_filtering") else {
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
                &yaml_path.join("phrase_filtering").join(&safe),
            ));
        }
        out
    }
}
