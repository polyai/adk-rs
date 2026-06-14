use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_parse::{ParseLocalResource, ResourceParseResult};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::phrase_filters::local::{
    PHRASE_FILTERS_FILE_PATH, PhraseFiltersFile, parse_phrase_filters_file,
};
use crate::resource_utils::{clean_name, rel_under_root};
use serde_yaml_ng::Value;
use std::path::Path;

// poly/resources/phrase_filter.py
/// Validation parity: implemented against Python PhraseFilter.validate().
pub(crate) struct PhraseFilter;
impl DiscoverResources for PhraseFilter {
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::InFile {
        path: PHRASE_FILTERS_FILE_PATH,
        yaml_path: &["phrase_filtering"],
    };

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let yaml_path = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if !is_file(fs, &yaml_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(fs, &yaml_path) else {
            return vec![];
        };
        let Some(Value::Sequence(list)) = m.get("phrase_filtering") else {
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
                &yaml_path.join("phrase_filtering").join(&safe),
            ));
        }
        out
    }

    fn append_local_resource_errors(_path: &str, yaml: &Value, errors: &mut Vec<String>) {
        <Self as ParseLocalResource>::append_parse_errors(
            Self::LOCAL_PATH.primary_path().expect("local file path"),
            yaml,
            errors,
        );
    }
}

#[cfg(test)]
pub(crate) fn append_parse_errors(yaml: &Value, errors: &mut Vec<String>) {
    let path = PhraseFilter::LOCAL_PATH
        .primary_path()
        .expect("local file path");
    <PhraseFilter as ParseLocalResource>::append_parse_errors(path, yaml, errors);
}

impl ParseLocalResource for PhraseFilter {
    type Parsed = PhraseFiltersFile;

    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed> {
        parse_phrase_filters_file(path, yaml)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml_ng::from_str;

    fn validation_errors(yaml: &str) -> Vec<String> {
        let yaml = from_str::<Value>(yaml).expect("phrase filter YAML");
        let mut errors = Vec::new();
        append_parse_errors(&yaml, &mut errors);
        errors
    }

    #[test]
    fn validates_python_phrase_filter_local_required_fields() {
        assert_eq!(
            PhraseFilter::LOCAL_PATH,
            LocalResourcePath::InFile {
                path: PHRASE_FILTERS_FILE_PATH,
                yaml_path: &["phrase_filtering"],
            }
        );

        let missing_name = validation_errors(
            r#"
phrase_filtering:
  - description: Missing all
"#,
        );
        assert!(
            missing_name
                .iter()
                .any(|error| error.contains("missing field `name`"))
        );

        let empty_regex = validation_errors(
            r#"
phrase_filtering:
  - name: Empty regex
    regular_expressions: []
"#,
        );

        assert!(
            empty_regex
                .iter()
                .any(|error| error.contains("At least one regular expression is required"))
        );
    }
}
