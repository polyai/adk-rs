//! Translation resource-family semantics.

mod command_gen;
mod discovery;
mod materialization;

pub(crate) use command_gen::{payload_json_summary, translation_lifecycle_commands};
pub(crate) use discovery::Translation;
pub(crate) use materialization::insert_translation_resources;

use crate::push_command_inputs::resource_yaml;
use crate::specs::{LANGUAGES_FILE, TRANSLATIONS};
use adk_types::ResourceMap;
use serde_yaml_ng::Value as YamlValue;
use std::collections::BTreeSet;

pub fn validate_language_translation_resources(resources: &ResourceMap) -> Vec<String> {
    let configured_languages = configured_languages(resources);
    if configured_languages.is_empty() {
        return Vec::new();
    }
    let Some(yaml) = resource_yaml(resources, TRANSLATIONS.file.file_path) else {
        return Vec::new();
    };
    let mut errors = Vec::new();
    for translation in crate::push_command_inputs::yaml_sequence(&yaml, TRANSLATIONS.yaml_key) {
        let name = crate::yaml_str(translation, "name");
        if name.is_empty() {
            continue;
        }
        let translation_languages = translation
            .get("translations")
            .and_then(YamlValue::as_mapping)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|(key, value)| value.as_str().and_then(|_| key.as_str()))
                    .map(ToString::to_string)
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();
        let missing = configured_languages
            .difference(&translation_languages)
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            errors.push(format!(
                "Validation error in {}/translations/{}: Missing translations for configured languages: {:?}.",
                TRANSLATIONS.file.file_path,
                crate::clean_name(&name, false),
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
                crate::clean_name(&name, false),
                extra
            ));
        }
    }
    errors
}

fn configured_languages(resources: &ResourceMap) -> BTreeSet<String> {
    let Some(yaml) = resource_yaml(resources, LANGUAGES_FILE.file_path) else {
        return BTreeSet::new();
    };
    let (default_language, additional_languages) =
        crate::languages::language_codes_from_yaml(&yaml);
    default_language
        .into_iter()
        .chain(additional_languages)
        .collect()
}
