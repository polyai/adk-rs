use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::{clean_name, rel_under_root};
use serde_yaml::Value;
use std::path::Path;

// poly/resources/keyphrase_boosting.py
pub(crate) struct KeyphraseBoosting;
impl DiscoverResources for KeyphraseBoosting {
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::InFile {
        path: crate::specs::KEYPHRASE_BOOSTING_FILE.file_path,
        yaml_path: &["keyphrases"],
    };

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let yaml_path = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if !is_file(fs, &yaml_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(fs, &yaml_path) else {
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
