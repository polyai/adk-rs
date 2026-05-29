use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::{clean_name, rel_under_root};
use serde_yaml_ng::Value;
use std::path::Path;

// poly/resources/pronunciation.py
pub(crate) struct Pronunciation;
impl DiscoverResources for Pronunciation {
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::InFile {
        path: crate::specs::PRONUNCIATIONS_FILE.file_path,
        yaml_path: &["pronunciations"],
    };

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let yaml_path = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if !is_file(fs, &yaml_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(fs, &yaml_path) else {
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
