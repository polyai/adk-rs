use crate::CommandGenError;
use crate::materialization::insert_yaml_resource;
use crate::push_command_inputs::json_str;
use crate::specs::TRANSLATIONS;
use adk_types::ResourceMap;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Serialize)]
struct TranslationsYaml {
    translations: Vec<TranslationYaml>,
}

#[derive(Serialize)]
struct TranslationYaml {
    name: String,
    translations: BTreeMap<String, String>,
}

pub(crate) fn insert_translation_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    let translations = TRANSLATIONS
        .owned_entries(projection)
        .into_iter()
        .filter_map(|(_, translation)| translation_yaml(&translation))
        .collect::<Vec<_>>();
    if translations.is_empty() {
        return Ok(());
    }

    insert_yaml_resource(
        map,
        TRANSLATIONS.file.file_path,
        TRANSLATIONS.file.resource_id,
        TRANSLATIONS.file.name,
        TranslationsYaml { translations },
    )
}

fn translation_yaml(value: &Value) -> Option<TranslationYaml> {
    let name = json_str(value, &["translationKey", "translation_key"]);
    if name.is_empty() {
        return None;
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
    Some(TranslationYaml { name, translations })
}
