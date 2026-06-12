use super::discovery::TranslationItem as LocalTranslationItem;
use crate::ids::stable_resource_id;
use crate::local_parse::ParseLocalResource;
use crate::push_command;
use crate::push_command_inputs::{SimpleLifecycleCommands, json_str, resource_yaml};
use crate::specs::TRANSLATIONS;
use adk_protobuf::Metadata;
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::translations::{
    LanguageHubTranslationsCreate, LanguageHubTranslationsDelete, LanguageHubTranslationsUpdate,
    LocalizedText, UpdateEntry,
};
use adk_types::ResourceMap;
use serde_json::{Value as JsonValue, json};
use serde_yaml_ng::Value as YamlValue;
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq)]
struct TranslationItem {
    id: String,
    translation_key: String,
    translations: BTreeMap<String, String>,
}

pub(crate) fn translation_lifecycle_commands(
    resources: &ResourceMap,
    projection: &JsonValue,
    metadata: &Option<Metadata>,
) -> SimpleLifecycleCommands {
    let Some(yaml) = resource_yaml(resources, TRANSLATIONS.file.file_path) else {
        return SimpleLifecycleCommands::default();
    };
    let local_items = local_translation_items(&yaml);
    let remote_items = remote_translation_items(projection);
    let local_keys = local_items
        .iter()
        .map(|item| item.translation_key.clone())
        .collect::<HashSet<_>>();
    let remote_by_key = remote_items
        .iter()
        .map(|item| (item.translation_key.clone(), item.clone()))
        .collect::<HashMap<_, _>>();

    let mut commands = SimpleLifecycleCommands::default();
    for remote in &remote_items {
        if !local_keys.contains(&remote.translation_key) {
            push_command(
                &mut commands.deletes,
                metadata,
                "delete_translation",
                CommandPayload::DeleteTranslation(LanguageHubTranslationsDelete {
                    id: remote.id.clone(),
                }),
            );
        }
    }
    for local in &local_items {
        match remote_by_key.get(&local.translation_key) {
            Some(remote) if local.translations != remote.translations => push_command(
                &mut commands.updates,
                metadata,
                "update_translation",
                CommandPayload::UpdateTranslation(LanguageHubTranslationsUpdate {
                    id: remote.id.clone(),
                    translation_key: Some(local.translation_key.clone()),
                    translations: update_entries(&local.translations),
                }),
            ),
            Some(_) => {}
            None => {
                let id = stable_resource_id(
                    TRANSLATIONS.id_prefix,
                    &local.translation_key,
                    TRANSLATIONS.file.file_path,
                );
                push_command(
                    &mut commands.creates,
                    metadata,
                    "create_translation",
                    CommandPayload::CreateTranslation(LanguageHubTranslationsCreate {
                        id,
                        translation_key: local.translation_key.clone(),
                        translations: localized_texts(&local.translations),
                    }),
                );
            }
        }
    }
    commands
}

pub(crate) fn payload_json_summary(payload: &CommandPayload) -> Option<(&'static str, JsonValue)> {
    match payload {
        CommandPayload::CreateTranslation(msg) => Some((
            "create_translation",
            json!({
                "id": msg.id,
                "translation_key": msg.translation_key,
                "translations": msg
                    .translations
                    .iter()
                    .map(localized_text_json)
                    .collect::<Vec<_>>(),
            }),
        )),
        CommandPayload::UpdateTranslation(msg) => Some((
            "update_translation",
            json!({
                "id": msg.id,
                "translation_key": msg.translation_key.clone().unwrap_or_default(),
                "translations": msg
                    .translations
                    .iter()
                    .map(update_entry_json)
                    .collect::<Vec<_>>(),
            }),
        )),
        CommandPayload::DeleteTranslation(msg) => Some((
            "delete_translation",
            json!({
                "id": msg.id,
            }),
        )),
        _ => None,
    }
}

fn local_translation_items(yaml: &YamlValue) -> Vec<TranslationItem> {
    let Ok(file) =
        crate::translations::Translation::parse_local_yaml(TRANSLATIONS.file.file_path, yaml)
    else {
        return Vec::new();
    };
    file.translations
        .iter()
        .map(local_translation_item)
        .collect()
}

fn local_translation_item(item: &LocalTranslationItem) -> TranslationItem {
    TranslationItem {
        id: String::new(),
        translation_key: item.name().to_string(),
        translations: item.translations().clone(),
    }
}

fn remote_translation_items(projection: &JsonValue) -> Vec<TranslationItem> {
    TRANSLATIONS
        .entries(projection)
        .into_iter()
        .filter_map(|(id, value)| {
            let translation_key = json_str(value, &["translationKey", "translation_key"]);
            if translation_key.is_empty() {
                return None;
            }
            Some(TranslationItem {
                id,
                translation_key,
                translations: remote_translations(value),
            })
        })
        .collect()
}

fn remote_translations(value: &JsonValue) -> BTreeMap<String, String> {
    value
        .get("translations")
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .filter_map(|localized| {
            let language_code = json_str(localized, &["languageCode", "language_code"]);
            if language_code.is_empty() {
                return None;
            }
            Some((language_code, json_str(localized, &["text"])))
        })
        .collect()
}

fn localized_texts(translations: &BTreeMap<String, String>) -> Vec<LocalizedText> {
    translations
        .iter()
        .map(|(language_code, text)| LocalizedText {
            language_code: language_code.clone(),
            text: text.clone(),
            is_auto_translated: false,
        })
        .collect()
}

fn update_entries(translations: &BTreeMap<String, String>) -> Vec<UpdateEntry> {
    translations
        .iter()
        .map(|(language_code, text)| UpdateEntry {
            language_code: language_code.clone(),
            text: Some(text.clone()),
            is_auto_translated: Some(false),
        })
        .collect()
}

fn localized_text_json(text: &LocalizedText) -> JsonValue {
    json!({
        "language_code": text.language_code,
        "text": text.text,
        "is_auto_translated": text.is_auto_translated,
    })
}

fn update_entry_json(entry: &UpdateEntry) -> JsonValue {
    json!({
        "language_code": entry.language_code,
        "text": entry.text.clone().unwrap_or_default(),
        "is_auto_translated": entry.is_auto_translated.unwrap_or(false),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_translation_items_use_typed_file_model() {
        let yaml = serde_yaml_ng::from_str(
            r#"
translations:
  - name: greeting
    translations:
      fr-FR: Bonjour
      en-US: Hello
"#,
        )
        .expect("translations yaml");

        let items = local_translation_items(&yaml);

        assert_eq!(items[0].translation_key, "greeting");
        assert_eq!(
            items[0].translations.keys().cloned().collect::<Vec<_>>(),
            vec!["en-US".to_string(), "fr-FR".to_string()]
        );
    }
}
