use crate::languages::local::{LanguagesFile, parse_languages_content};
use crate::push_command;
use crate::push_command_inputs::{SimpleLifecycleCommands, json_str};
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
    let Some(file) = local_languages_file(resources) else {
        return;
    };
    let Some(local) = file.default_language().map(ToString::to_string) else {
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
    let Some(file) = local_languages_file(resources) else {
        return SimpleLifecycleCommands::default();
    };
    let local_languages = local_additional_languages(&file);
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

fn local_languages_file(resources: &ResourceMap) -> Option<LanguagesFile> {
    let content = resources
        .get(LANGUAGES_FILE.file_path)?
        .payload
        .get("content")?
        .as_str()?;
    parse_languages_content(LANGUAGES_FILE.file_path, content).ok()
}

fn local_additional_languages(file: &LanguagesFile) -> Vec<String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use adk_types::Resource;

    fn language_resource(content: &str) -> ResourceMap {
        let mut resources = ResourceMap::new();
        resources.insert(
            LANGUAGES_FILE.file_path.to_string(),
            Resource {
                resource_id: LANGUAGES_FILE.resource_id.to_string(),
                name: LANGUAGES_FILE.name.to_string(),
                file_path: LANGUAGES_FILE.file_path.to_string(),
                payload: json!({ "content": content }),
            },
        );
        resources
    }

    #[test]
    fn language_commands_parse_local_content_through_typed_model() {
        let resources = language_resource(
            r#"
default_language: en-US
additional_languages:
  - fr-FR
  - es-ES
"#,
        );
        let projection = json!({
            "languages": {
                "defaultLanguageCode": "en-GB",
                "additionalLanguages": {
                    "ids": ["fr-FR", "de-DE"],
                    "entities": {
                        "fr-FR": { "code": "fr-FR" },
                        "de-DE": { "code": "de-DE" }
                    }
                }
            }
        });

        let mut updates = Vec::new();
        append_default_language_update(&mut updates, &resources, &projection, &None);
        let lifecycle = additional_language_lifecycle_commands(&resources, &projection, &None);

        assert_eq!(updates.len(), 1);
        assert!(matches!(
            updates[0].payload,
            Some(CommandPayload::LanguagesUpdateDefaultLanguage(
                LanguagesUpdateDefaultLanguage { ref language_code }
            )) if language_code == "en-US"
        ));
        assert_eq!(lifecycle.creates.len(), 1);
        assert!(matches!(
            lifecycle.creates[0].payload,
            Some(CommandPayload::LanguagesAddLanguage(LanguagesAddLanguage { ref code }))
                if code == "es-ES"
        ));
        assert_eq!(lifecycle.deletes.len(), 1);
        assert!(matches!(
            lifecycle.deletes[0].payload,
            Some(CommandPayload::LanguagesDeleteLanguage(LanguagesDeleteLanguage { ref code }))
                if code == "de-DE"
        ));
    }

    #[test]
    fn parse_errors_do_not_delete_remote_additional_languages() {
        let resources = language_resource("additional_languages: definitely not a list\n");
        let projection = json!({
            "languages": {
                "additionalLanguages": {
                    "ids": ["fr-FR"],
                    "entities": {
                        "fr-FR": { "code": "fr-FR" }
                    }
                }
            }
        });

        let mut updates = Vec::new();
        append_default_language_update(&mut updates, &resources, &projection, &None);
        let lifecycle = additional_language_lifecycle_commands(&resources, &projection, &None);

        assert!(updates.is_empty());
        assert!(lifecycle.creates.is_empty());
        assert!(lifecycle.deletes.is_empty());
        assert!(lifecycle.updates.is_empty());
    }
}
