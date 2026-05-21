use crate::discover::DiscoverResources;
use crate::discover::resource_utils::{clean_name, rel_under_root};
use crate::resources::common::{is_file, read_yaml_mapping};
use serde_yaml::Value;
use std::path::Path;

// poly/resources/transcript_correction.py
pub(crate) struct TranscriptCorrection;
impl DiscoverResources for TranscriptCorrection {
    const TYPE_NAME: &'static str = "TranscriptCorrection";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let yaml_path = base_path.join("voice/speech_recognition/transcript_corrections.yaml");
        if !is_file(&yaml_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(&yaml_path) else {
            return vec![];
        };
        let Some(Value::Sequence(list)) = m.get("corrections") else {
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
                &yaml_path.join("corrections").join(&safe),
            ));
        }
        out
    }
}
