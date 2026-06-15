use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::languages::local::{LanguagesFile, parse_languages_file};
use crate::local_parse::{ParseLocalResource, ResourceParseResult};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::rel_under_root;
use serde_yaml_ng::Value;
use std::path::Path;

/// Validation parity: implemented against Python DefaultLanguage.validate().
pub(crate) struct DefaultLanguage;
impl DiscoverResources for DefaultLanguage {
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::InFile {
        path: crate::specs::LANGUAGES_FILE.file_path,
        yaml_path: &["default_language"],
    };

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let yaml_path = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if !is_file(fs, &yaml_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(fs, &yaml_path) else {
            return vec![];
        };
        match m.get("default_language").and_then(Value::as_str) {
            Some(value) if !value.is_empty() => vec![rel_under_root(
                base_path,
                &yaml_path.join("default_language"),
            )],
            _ => vec![],
        }
    }

    fn append_local_resource_errors(path: &str, yaml: &Value, errors: &mut Vec<String>) {
        <Self as ParseLocalResource>::append_parse_errors(path, yaml, errors);
    }
}

impl ParseLocalResource for DefaultLanguage {
    type Parsed = LanguagesFile;

    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed> {
        parse_languages_file(path, yaml)
    }
}

/// Validation parity: implemented against Python AdditionalLanguage.validate().
pub(crate) struct AdditionalLanguage;
impl DiscoverResources for AdditionalLanguage {
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::InFile {
        path: crate::specs::LANGUAGES_FILE.file_path,
        yaml_path: &["additional_languages"],
    };

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let yaml_path = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if !is_file(fs, &yaml_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(fs, &yaml_path) else {
            return vec![];
        };
        m.get("additional_languages")
            .and_then(Value::as_sequence)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|code| !code.is_empty())
            .map(|code| {
                rel_under_root(
                    base_path,
                    &yaml_path.join("additional_languages").join(code),
                )
            })
            .collect()
    }
}
