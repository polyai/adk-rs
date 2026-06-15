use crate::CommandGenError;
use crate::materialization::to_yaml_string;
use crate::push_command_inputs::json_str;
use crate::specs::TRANSLATIONS;
use crate::translations::local::{TranslationItem, TranslationsFile};
use adk_types::ResourceMap;
use serde_json::Value;
use std::collections::BTreeMap;

pub(crate) fn insert_translation_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    let translations = TRANSLATIONS
        .owned_entries(projection)
        .into_iter()
        .filter_map(|(_, translation)| local_translation_from_projection(&translation).transpose())
        .collect::<Result<Vec<_>, _>>()?;
    if translations.is_empty() {
        return Ok(());
    }

    let content = to_yaml_string(&TranslationsFile::new(translations))
        .map_err(|error| CommandGenError::InvalidData(error.to_string()))?;
    crate::materialization::insert_content_resource(
        map,
        TRANSLATIONS.file.file_path,
        TRANSLATIONS.file.resource_id,
        TRANSLATIONS.file.name,
        content,
    )
}

fn local_translation_from_projection(
    value: &Value,
) -> Result<Option<TranslationItem>, CommandGenError> {
    let name = json_str(value, &["translationKey", "translation_key"]);
    if name.is_empty() {
        return Ok(None);
    }
    let translations = value
        .get("translations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|localized| {
            let language_code = json_str(localized, &["languageCode", "language_code"]);
            if language_code.is_empty() {
                return None;
            }
            Some((language_code, json_str(localized, &["text"])))
        })
        .collect::<BTreeMap<_, _>>();
    TranslationItem::from_projection(name, translations)
        .map(Some)
        .map_err(invalid_translation_projection)
}

fn invalid_translation_projection(error: String) -> CommandGenError {
    CommandGenError::InvalidData(format!("Invalid translation projection: {error}"))
}
