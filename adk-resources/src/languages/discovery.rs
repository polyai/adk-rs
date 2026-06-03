use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::{is_valid_language_code, rel_under_root};
use serde_yaml_ng::Value;
use std::collections::BTreeSet;
use std::path::Path;

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

    fn validate_local_yaml(_path: &str, yaml: &Value, errors: &mut Vec<String>) {
        validate_languages_yaml(yaml, errors);
    }
}

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

pub(crate) fn language_codes_from_yaml(yaml: &Value) -> (Option<String>, Vec<String>) {
    let default_language = yaml
        .get("default_language")
        .and_then(Value::as_str)
        .filter(|code| !code.is_empty())
        .map(ToString::to_string);
    let additional_languages = yaml
        .get("additional_languages")
        .and_then(Value::as_sequence)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|code| !code.is_empty())
        .map(ToString::to_string)
        .collect();
    (default_language, additional_languages)
}

fn validate_languages_yaml(yaml: &Value, errors: &mut Vec<String>) {
    let path = crate::specs::LANGUAGES_FILE.file_path;
    let (default_language, additional_languages) = language_codes_from_yaml(yaml);
    if let Some(default_language) = default_language.as_deref() {
        validate_language_code(
            path,
            "default_language",
            default_language,
            "Invalid language code",
            errors,
        );
    }

    let mut seen = BTreeSet::new();
    let mut duplicates = BTreeSet::new();
    for code in &additional_languages {
        validate_language_code(
            path,
            &format!("additional_languages/{code}"),
            code,
            "Invalid language code",
            errors,
        );
        if !seen.insert(code.clone()) {
            duplicates.insert(code.clone());
        }
    }
    for duplicate in duplicates {
        errors.push(format!(
            "Validation error in {path}/additional_languages/{duplicate}: Duplicate language code: '{duplicate}'."
        ));
    }
    if let Some(default_language) = default_language
        && additional_languages.contains(&default_language)
    {
        errors.push(format!(
            "Validation error in {path}/default_language: Default language '{default_language}' also appears in additional languages."
        ));
    }
}

fn validate_language_code(
    path: &str,
    logical_key: &str,
    code: &str,
    label: &str,
    errors: &mut Vec<String>,
) {
    if !is_valid_language_code(code) {
        errors.push(format!(
            "Validation error in {path}/{logical_key}: {label}: '{code}'. Must be a valid BCP 47 language tag."
        ));
    }
}
