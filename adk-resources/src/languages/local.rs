use crate::local_parse::{
    ResourceParseErrors, ResourceParseResult, default_if_null, deserialize_yaml, duplicate_names,
};
use crate::resource_utils::is_valid_language_code;
use serde::{Deserialize, Serialize};
use serde_yaml_ng::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct LanguagesFile {
    #[serde(default)]
    default_language: Option<String>,
    #[serde(default, deserialize_with = "default_if_null")]
    additional_languages: Vec<String>,
}

impl LanguagesFile {
    pub(crate) fn new(default_language: Option<String>, additional_languages: Vec<String>) -> Self {
        Self {
            default_language,
            additional_languages,
        }
    }

    pub(crate) fn default_language(&self) -> Option<&str> {
        self.default_language
            .as_deref()
            .filter(|code| !code.is_empty())
    }

    pub(crate) fn additional_languages(&self) -> impl Iterator<Item = &str> {
        self.additional_languages
            .iter()
            .map(String::as_str)
            .filter(|code| !code.is_empty())
    }

    pub(crate) fn language_codes(&self) -> (Option<String>, Vec<String>) {
        (
            self.default_language().map(ToString::to_string),
            self.additional_languages()
                .map(ToString::to_string)
                .collect(),
        )
    }

    fn validate(self, path: &str) -> ResourceParseResult<Self> {
        let mut errors = ResourceParseErrors::new();
        if let Some(default_language) = self.default_language() {
            validate_language_code(&mut errors, path, "default_language", default_language);
        }

        let additional_languages = self
            .additional_languages()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        for code in &additional_languages {
            validate_language_code(
                &mut errors,
                path,
                &format!("additional_languages/{code}"),
                code,
            );
        }
        for duplicate in duplicate_names(additional_languages.iter().map(String::as_str)) {
            errors.push(
                &format!("{path}/additional_languages/{duplicate}"),
                format!("Duplicate language code: '{duplicate}'."),
            );
        }
        if let Some(default_language) = self.default_language()
            && additional_languages
                .iter()
                .any(|code| code == default_language)
        {
            errors.push(
                &format!("{path}/default_language"),
                format!(
                    "Default language '{default_language}' also appears in additional languages."
                ),
            );
        }

        if errors.is_empty() {
            Ok(self)
        } else {
            Err(errors)
        }
    }
}

pub(crate) fn parse_languages_file(path: &str, yaml: &Value) -> ResourceParseResult<LanguagesFile> {
    deserialize_yaml::<LanguagesFile>(path, yaml)?.validate(path)
}

pub(crate) fn parse_languages_content(
    path: &str,
    content: &str,
) -> ResourceParseResult<LanguagesFile> {
    let yaml = serde_yaml_ng::from_str::<Value>(content)
        .map_err(|error| ResourceParseErrors::single(path, error))?;
    parse_languages_file(path, &yaml)
}

pub(crate) fn language_codes_from_content(content: &str) -> (Option<String>, Vec<String>) {
    parse_languages_content(crate::specs::LANGUAGES_FILE.file_path, content)
        .map(|file| file.language_codes())
        .unwrap_or_default()
}

fn validate_language_code(
    errors: &mut ResourceParseErrors,
    path: &str,
    logical_key: &str,
    code: &str,
) {
    if !is_valid_language_code(code) {
        errors.push(
            &format!("{path}/{logical_key}"),
            format!("Invalid language code: '{code}'. Must be a valid BCP 47 language tag."),
        );
    }
}
