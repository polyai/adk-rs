use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_parse::{ParseLocalResource, ResourceParseResult};
use crate::local_resources::{is_dir, sorted_read_dir};
use crate::resource_utils::rel_under_root;
use crate::topics::local::{LocalTopic, parse_topic_file};
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

    fn append_local_resource_errors(path: &str, yaml: &Value, errors: &mut Vec<String>) {
        <Self as ParseLocalResource>::append_parse_errors(path, yaml, errors);
    }
}

impl ParseLocalResource for Topic {
    type Parsed = LocalTopic;

    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed> {
        parse_topic_file(path, yaml)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml_ng::from_str;

    fn validation_errors(path: &str, yaml: &str) -> Vec<String> {
        let yaml = from_str::<Value>(yaml).expect("topic YAML");
        let mut errors = Vec::new();
        Topic::append_local_resource_errors(path, &yaml, &mut errors);
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

        let invalid_reference_type = validation_errors(
            "topics/invalid_topic.yaml",
            r#"
name: Invalid Topic
actions: Use {{ft:flow_function}}
content: ""
example_queries: []
"#,
        );
        assert!(invalid_reference_type.iter().any(|error| error.contains(
            "Invalid reference type: transition_functions is not a valid reference type"
        )));

        assert!(
            validation_errors(
                "topics/name_from_file.yaml",
                r#"
actions: ""
content: ""
example_queries: []
"#
            )
            .is_empty()
        );
    }
}
