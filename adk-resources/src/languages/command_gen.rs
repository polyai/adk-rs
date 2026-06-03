use crate::push_command_inputs::{SimpleLifecycleCommands, json_str, resource_yaml, yaml_sequence};
use crate::specs::{ADDITIONAL_LANGUAGES, LANGUAGES_FILE};
use crate::{push_command, yaml_str};
use adk_protobuf::Metadata;
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::languages::{
    LanguagesAddLanguage, LanguagesDeleteLanguage, LanguagesUpdateDefaultLanguage,
};
use adk_types::ResourceMap;
use serde_json::Value as JsonValue;
use serde_yaml_ng::Value as YamlValue;
use std::collections::{HashMap, HashSet};

pub(crate) fn append_default_language_update(
    updates: &mut Vec<adk_protobuf::Command>,
    resources: &ResourceMap,
    projection: &JsonValue,
    metadata: &Option<Metadata>,
) {
    let Some(local) = local_default_language(resources) else {
        return;
    };
    let remote = projection
        .pointer("/languages/defaultLanguageCode")
        .or_else(|| projection.pointer("/languages/defaultLanguage"))
        .and_then(JsonValue::as_str)
        .unwrap_or("");
    if local != remote {
        push_command(
            updates,
            metadata,
            "languages_update_default_language",
            CommandPayload::LanguagesUpdateDefaultLanguage(LanguagesUpdateDefaultLanguage {
                language_code: local,
            }),
        );
    }
}

pub(crate) fn additional_language_lifecycle_commands(
    resources: &ResourceMap,
    projection: &JsonValue,
    metadata: &Option<Metadata>,
) -> SimpleLifecycleCommands {
    let Some(yaml) = resource_yaml(resources, LANGUAGES_FILE.file_path) else {
        return SimpleLifecycleCommands::default();
    };
    let local_languages = local_additional_languages(&yaml);
    let remote_languages = remote_additional_languages(projection);
    let local_set = local_languages.iter().cloned().collect::<HashSet<_>>();
    let remote_set = remote_languages.keys().cloned().collect::<HashSet<_>>();

    let mut commands = SimpleLifecycleCommands::default();
    for code in remote_set.difference(&local_set) {
        push_command(
            &mut commands.deletes,
            metadata,
            "languages_delete_language",
            CommandPayload::LanguagesDeleteLanguage(LanguagesDeleteLanguage { code: code.clone() }),
        );
    }
    for code in local_set.difference(&remote_set) {
        push_command(
            &mut commands.creates,
            metadata,
            "languages_add_language",
            CommandPayload::LanguagesAddLanguage(LanguagesAddLanguage { code: code.clone() }),
        );
    }
    commands
}

fn local_default_language(resources: &ResourceMap) -> Option<String> {
    resource_yaml(resources, LANGUAGES_FILE.file_path).and_then(|yaml| {
        let code = yaml_str(&yaml, "default_language");
        (!code.is_empty()).then_some(code)
    })
}

fn local_additional_languages(yaml: &YamlValue) -> Vec<String> {
    yaml_sequence(yaml, ADDITIONAL_LANGUAGES.yaml_key)
        .into_iter()
        .filter_map(YamlValue::as_str)
        .filter(|code| !code.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn remote_additional_languages(projection: &JsonValue) -> HashMap<String, String> {
    ADDITIONAL_LANGUAGES
        .entries(projection)
        .into_iter()
        .filter_map(|(id, value)| {
            let code = json_str(value, &["code"]);
            let code = if code.is_empty() { id.clone() } else { code };
            (!code.is_empty()).then_some((code, id))
        })
        .collect()
}
