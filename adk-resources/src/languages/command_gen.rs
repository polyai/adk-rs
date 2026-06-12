use crate::languages::DefaultLanguage;
use crate::local_parse::ParseLocalResource;
use crate::push_command;
use crate::push_command_inputs::{SimpleLifecycleCommands, json_str, resource_yaml};
use crate::specs::{ADDITIONAL_LANGUAGES, LANGUAGES_FILE};
use adk_protobuf::Metadata;
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::languages::{
    LanguagesAddLanguage, LanguagesDeleteLanguage, LanguagesUpdateDefaultLanguage,
};
use adk_types::ResourceMap;
use serde_json::{Value as JsonValue, json};
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

pub(crate) fn payload_json_summary(payload: &CommandPayload) -> Option<(&'static str, JsonValue)> {
    match payload {
        CommandPayload::LanguagesUpdateDefaultLanguage(msg) => Some((
            "languages_update_default_language",
            json!({
                "language_code": msg.language_code,
            }),
        )),
        CommandPayload::LanguagesAddLanguage(msg) => Some((
            "languages_add_language",
            json!({
                "code": msg.code,
            }),
        )),
        CommandPayload::LanguagesDeleteLanguage(msg) => Some((
            "languages_delete_language",
            json!({
                "code": msg.code,
            }),
        )),
        _ => None,
    }
}

fn local_default_language(resources: &ResourceMap) -> Option<String> {
    let yaml = resource_yaml(resources, LANGUAGES_FILE.file_path)?;
    let file = DefaultLanguage::parse_local_yaml(LANGUAGES_FILE.file_path, &yaml).ok()?;
    file.default_language().map(ToString::to_string)
}

fn local_additional_languages(yaml: &serde_yaml_ng::Value) -> Vec<String> {
    let Ok(file) = DefaultLanguage::parse_local_yaml(LANGUAGES_FILE.file_path, yaml) else {
        return Vec::new();
    };
    file.additional_languages()
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
