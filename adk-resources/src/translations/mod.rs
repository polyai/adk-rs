//! Translation resource-family semantics.

mod command_gen;
mod discovery;
mod local;
mod materialization;

pub(crate) use command_gen::{payload_json_summary, translation_lifecycle_commands};
pub(crate) use discovery::Translation;
pub(crate) use materialization::insert_translation_resources;

use crate::specs::{LANGUAGES_FILE, TRANSLATIONS};
use crate::translations::local::parse_translation_language_coverage_content;
use adk_types::ResourceMap;
use std::collections::BTreeSet;

pub fn validate_language_translation_resources(resources: &ResourceMap) -> Vec<String> {
    let configured_languages = configured_languages(resources);
    if configured_languages.is_empty() {
        return Vec::new();
    }
    let Some(content) = resource_content(resources, TRANSLATIONS.file.file_path) else {
        return Vec::new();
    };
    let Ok(file) =
        parse_translation_language_coverage_content(TRANSLATIONS.file.file_path, content)
    else {
        return Vec::new();
    };
    let mut errors = Vec::new();
    for translation in &file.translations {
        let name = translation.name();
        if name.is_empty() {
            continue;
        }
        let translation_languages = translation.translated_languages();
        let missing = configured_languages
            .difference(&translation_languages)
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            errors.push(format!(
                "Validation error in {}/translations/{}: Missing translations for configured languages: {:?}.",
                TRANSLATIONS.file.file_path,
                crate::clean_name(name, false),
                missing
            ));
        }
        let extra = translation_languages
            .difference(&configured_languages)
            .cloned()
            .collect::<Vec<_>>();
        if !extra.is_empty() {
            errors.push(format!(
                "Validation error in {}/translations/{}: Translation for language not configured: {:?}.",
                TRANSLATIONS.file.file_path,
                crate::clean_name(name, false),
                extra
            ));
        }
    }
    errors
}

fn configured_languages(resources: &ResourceMap) -> BTreeSet<String> {
    let Some(content) = resource_content(resources, LANGUAGES_FILE.file_path) else {
        return BTreeSet::new();
    };
    let (default_language, additional_languages) =
        crate::languages::language_codes_from_content(content);
    default_language
        .into_iter()
        .chain(additional_languages)
        .collect()
}

fn resource_content<'a>(resources: &'a ResourceMap, path: &str) -> Option<&'a str> {
    resources.get(path)?.payload.get("content")?.as_str()
}
