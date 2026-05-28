use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_resources::{is_dir, sorted_read_dir};
use crate::resource_utils::rel_under_root;
use std::path::Path;

// poly/resources/topic.py
pub(crate) struct Topic;
impl DiscoverResources for Topic {
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::Directory("topics");

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let topics = base_path.join(Self::LOCAL_PATH.primary_path().expect("local directory"));
        if !is_dir(fs, &topics) {
            return vec![];
        }
        let mut out = Vec::new();
        if let Some(files) = sorted_read_dir(fs, &topics) {
            for f in files {
                if let Some(ext) = f.extension().and_then(|e| e.to_str())
                    && (ext == "yaml" || ext == "yml")
                {
                    out.push(rel_under_root(base_path, &f));
                }
            }
        }
        out
    }

    fn validate_local_yaml(path: &str, yaml: &serde_yaml::Value, errors: &mut Vec<String>) {
        validate_local_yaml(path, yaml, errors);
    }
}

pub(crate) fn validate_local_yaml(path: &str, yaml: &serde_yaml::Value, errors: &mut Vec<String>) {
    if yaml
        .get("name")
        .and_then(serde_yaml::Value::as_str)
        .is_none_or(str::is_empty)
    {
        errors.push(format!(
            "Validation error in {path}: topic name is required."
        ));
    }
}
