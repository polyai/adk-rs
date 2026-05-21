use crate::discover::DiscoverResources;
use crate::resource_utils::rel_under_root;
use crate::resources::common::{is_dir, sorted_read_dir};
use std::path::Path;

// poly/resources/topic.py
pub(crate) struct Topic;
impl DiscoverResources for Topic {
    fn discover_resources(base_path: &Path) -> Vec<String> {
        let topics = base_path.join("topics");
        if !is_dir(&topics) {
            return vec![];
        }
        let mut out = Vec::new();
        if let Some(files) = sorted_read_dir(&topics) {
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
