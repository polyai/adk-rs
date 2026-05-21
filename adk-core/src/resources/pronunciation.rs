use crate::discover::DiscoverResources;
use crate::resource_utils::{clean_name, rel_under_root};
use crate::resources::common::{is_file, read_yaml_mapping};
use serde_yaml::Value;
use std::path::Path;

// poly/resources/pronunciation.py
pub(crate) struct Pronunciation;
impl DiscoverResources for Pronunciation {
    fn discover_resources(base_path: &Path) -> Vec<String> {
        let yaml_path = base_path.join("voice/response_control/pronunciations.yaml");
        if !is_file(&yaml_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(&yaml_path) else {
            return vec![];
        };
        let Some(Value::Sequence(items)) = m.get("pronunciations") else {
            return vec![];
        };
        let mut out = Vec::new();
        for (i, _item) in items.iter().enumerate() {
            let safe = clean_name(&i.to_string(), false);
            out.push(rel_under_root(
                base_path,
                &yaml_path.join("pronunciations").join(&safe),
            ));
        }
        out
    }
}
