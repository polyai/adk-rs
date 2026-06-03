use crate::ids::stable_resource_id;
use crate::push_command_inputs::{
    SimpleLifecycleCommands, json_str, resource_yaml, yaml_sequence, yaml_string_map,
};
use crate::specs::TRANSLATIONS;
use crate::{push_command, yaml_str};
use adk_protobuf::Metadata;
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::translations::{
    LanguageHubTranslationsCreate, LanguageHubTranslationsDelete, LanguageHubTranslationsUpdate,
    LocalizedText, UpdateEntry,
};
use adk_types::ResourceMap;
use serde_json::Value as JsonValue;
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

fn local_translation_items(yaml: &YamlValue) -> Vec<TranslationItem> {
    yaml_sequence(yaml, TRANSLATIONS.yaml_key)
        .into_iter()
        .filter_map(|item| {
            let translation_key = yaml_str(item, "name");
            if translation_key.is_empty() {
                return None;
            }
            Some(TranslationItem {
                id: String::new(),
                translation_key,
                translations: yaml_string_map(item.get("translations"))
                    .into_iter()
                    .collect(),
            })
        })
        .collect()
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
