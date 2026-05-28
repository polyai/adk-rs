use crate::discover::DiscoverResources;
use crate::local_resources::{is_file, read_yaml_mapping, validate_named_sequence};
use crate::resource_utils::{clean_name, rel_under_root};
use serde_yaml::Value;
use std::path::Path;

// poly/resources/handoff.py
pub(crate) struct Handoff;
impl DiscoverResources for Handoff {
    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let path = base_path.join("config/handoffs.yaml");
        if !is_file(fs, &path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(fs, &path) else {
            return vec![];
        };
        let Some(Value::Sequence(list)) = m.get("handoffs") else {
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
                &path.join("handoffs").join(&safe),
            ));
        }
        out
    }
}

pub(crate) fn validate_local_yaml(yaml: &serde_yaml::Value, errors: &mut Vec<String>) {
    validate_named_sequence("config/handoffs.yaml", yaml, "handoffs", "handoff", errors);
}
