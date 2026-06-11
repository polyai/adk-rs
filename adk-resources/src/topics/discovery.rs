use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_resources::{is_dir, sorted_read_dir};
use crate::resource_utils::{clean_name, rel_under_root};
use serde_yaml_ng::Value;
use std::path::Path;

// poly/resources/topic.py
/// Validation parity: TODO(DEVP-319) audit Python Topic.validate().
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

    fn validate_local_yaml(path: &str, yaml: &Value, errors: &mut Vec<String>) {
        validate_local_yaml(path, yaml, errors);
    }
}

pub(crate) fn validate_local_yaml(path: &str, yaml: &Value, errors: &mut Vec<String>) {
    let name = yaml.get("name").and_then(Value::as_str).unwrap_or_default();
    if name.is_empty() {
        errors.push(format!(
            "Validation error in {path}: topic name is required."
        ));
    } else if let Some(file_stem) = Path::new(path).file_stem().and_then(|stem| stem.to_str()) {
        let expected_file_name = clean_name(name, true);
        if file_stem != expected_file_name {
            errors.push(format!(
                "Validation error in {path}: Topic name '{name}' in file {file_stem}.yaml does not match expected filename: {expected_file_name}.yaml"
            ));
        }
    }
    if let Some(example_queries) = yaml.get("example_queries").and_then(Value::as_sequence)
        && example_queries.len() > 20
    {
        errors.push(format!(
            "Validation error in {path}: Example queries must be less than 20"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml_ng::from_str;

    fn validation_errors(path: &str, yaml: &str) -> Vec<String> {
        let yaml = from_str::<Value>(yaml).expect("topic YAML");
        let mut errors = Vec::new();
        validate_local_yaml(path, &yaml, &mut errors);
        errors
    }

    #[test]
    fn validates_python_topic_filename_and_example_query_rules() {
        let errors = validation_errors(
            "topics/not_the_topic.yaml",
            r#"
name: Support Topic!
example_queries:
  - q01
  - q02
  - q03
  - q04
  - q05
  - q06
  - q07
  - q08
  - q09
  - q10
  - q11
  - q12
  - q13
  - q14
  - q15
  - q16
  - q17
  - q18
  - q19
  - q20
  - q21
"#,
        );

        assert!(
            errors
                .iter()
                .any(|error| error.contains("does not match expected filename: support_topic.yaml"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("Example queries must be less than 20"))
        );
    }
}
