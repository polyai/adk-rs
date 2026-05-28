use crate::discover::DiscoverResources;
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::{clean_name, rel_under_root};
use serde_yaml::Value;
use std::path::Path;

// poly/resources/keyphrase_boosting.py
pub(crate) struct KeyphraseBoosting;
impl DiscoverResources for KeyphraseBoosting {
    fn discover_resources(base_path: &Path) -> Vec<String> {
        let candidates = [
            base_path.join("voice/speech_recognition/keyphrase_boosting.yaml"),
            base_path.join("speech_recognition/keyphrase_boosting.yaml"),
        ];
        let yaml_path = candidates.into_iter().find(|p| is_file(p));
        let Some(yaml_path) = yaml_path else {
            return vec![];
        };
        let Some(m) = read_yaml_mapping(&yaml_path) else {
            return vec![];
        };
        let Some(Value::Sequence(list)) = m.get("keyphrases") else {
            return vec![];
        };
        let mut out = Vec::new();
        for item in list {
            let Value::Mapping(map) = item else { continue };
            let Some(name) = map.get("keyphrase").and_then(|v| v.as_str()) else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            let safe = clean_name(name, false);
            out.push(rel_under_root(
                base_path,
                &yaml_path.join("keyphrases").join(&safe),
            ));
        }
        out
    }
}
