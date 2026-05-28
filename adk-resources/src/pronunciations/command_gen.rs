use crate::ids::stable_resource_id;
use crate::push_commands::input_helpers::{
    SimpleLifecycleCommands, json_bool, json_i32, json_str, resource_yaml, yaml_bool, yaml_sequence,
};
use crate::specs::PRONUNCIATIONS;
use crate::{push_command, yaml_str};
use adk_protobuf::Metadata;
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::pronunciations::{
    PronunciationsCreatePronunciation, PronunciationsDeletePronunciation,
    PronunciationsUpdatePronunciation,
};
use adk_types::ResourceMap;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq)]
struct PronunciationItem {
    id: String,
    regex: String,
    replacement: String,
    case_sensitive: bool,
    language_code: String,
    description: String,
    position: i32,
    name: String,
}

pub(crate) fn pronunciation_lifecycle_commands(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> SimpleLifecycleCommands {
    let Some(yaml) = resource_yaml(resources, PRONUNCIATIONS.file.file_path) else {
        return SimpleLifecycleCommands::default();
    };
    let local_items = local_pronunciation_items(&yaml);
    let remote_items = remote_pronunciation_items(projection);
    let local_positions = local_items
        .iter()
        .map(|item| item.position)
        .collect::<HashSet<_>>();
    let remote_by_position = remote_items
        .iter()
        .map(|item| (item.position, item.clone()))
        .collect::<HashMap<_, _>>();

    let mut commands = SimpleLifecycleCommands::default();
    for remote in &remote_items {
        if !local_positions.contains(&remote.position) {
            push_command(
                &mut commands.deletes,
                metadata,
                "pronunciations_delete_pronunciation",
                CommandPayload::PronunciationsDeletePronunciation(
                    PronunciationsDeletePronunciation {
                        id: remote.id.clone(),
                    },
                ),
            );
        }
    }
    for local in &local_items {
        match remote_by_position.get(&local.position) {
            Some(remote) if pronunciation_item_needs_update(local, remote) => push_command(
                &mut commands.updates,
                metadata,
                "pronunciations_update_pronunciation",
                CommandPayload::PronunciationsUpdatePronunciation(
                    PronunciationsUpdatePronunciation {
                        id: Some(remote.id.clone()),
                        regex: Some(local.regex.clone()),
                        replacement: Some(local.replacement.clone()),
                        case_sensitive: Some(local.case_sensitive),
                        language_code: Some(local.language_code.clone()),
                        description: Some(local.description.clone()),
                        position: Some(local.position),
                        name: Some(local.name.clone()),
                    },
                ),
            ),
            Some(_) => {}
            None => {
                let id = stable_resource_id(
                    PRONUNCIATIONS.id_prefix,
                    &local.regex,
                    PRONUNCIATIONS.file.file_path,
                );
                push_command(
                    &mut commands.creates,
                    metadata,
                    "pronunciations_create_pronunciation",
                    CommandPayload::PronunciationsCreatePronunciation(
                        PronunciationsCreatePronunciation {
                            id,
                            regex: local.regex.clone(),
                            replacement: local.replacement.clone(),
                            case_sensitive: local.case_sensitive,
                            language_code: local.language_code.clone(),
                            description: local.description.clone(),
                            position: local.position,
                            name: local.name.clone(),
                        },
                    ),
                );
            }
        }
    }
    commands
}

fn local_pronunciation_items(yaml: &serde_yaml::Value) -> Vec<PronunciationItem> {
    yaml_sequence(yaml, PRONUNCIATIONS.yaml_key)
        .into_iter()
        .enumerate()
        .filter_map(|(idx, item)| {
            let regex = yaml_str(item, "regex");
            if regex.is_empty() {
                return None;
            }
            Some(PronunciationItem {
                id: String::new(),
                regex,
                replacement: yaml_str(item, "replacement"),
                case_sensitive: yaml_bool(item, "case_sensitive"),
                language_code: yaml_str(item, "language_code"),
                description: yaml_str(item, "description"),
                position: item
                    .get("position")
                    .and_then(serde_yaml::Value::as_i64)
                    .and_then(|value| i32::try_from(value).ok())
                    .unwrap_or(idx as i32),
                name: yaml_str(item, "name"),
            })
        })
        .collect()
}

fn remote_pronunciation_items(projection: &Value) -> Vec<PronunciationItem> {
    PRONUNCIATIONS
        .entries(projection)
        .into_iter()
        .filter_map(|(id, value)| {
            let regex = json_str(value, &["regex"]);
            if regex.is_empty() {
                return None;
            }
            Some(PronunciationItem {
                id,
                regex,
                replacement: json_str(value, &["replacement"]),
                case_sensitive: json_bool(value, &["caseSensitive", "case_sensitive"]),
                language_code: json_str(value, &["languageCode", "language_code"]),
                description: json_str(value, &["description"]),
                position: json_i32(value, &["position"]),
                name: json_str(value, &["name"]),
            })
        })
        .collect()
}

fn pronunciation_item_needs_update(local: &PronunciationItem, remote: &PronunciationItem) -> bool {
    local.regex != remote.regex
        || local.replacement != remote.replacement
        || local.case_sensitive != remote.case_sensitive
        || local.language_code != remote.language_code
        || local.description != remote.description
        || local.position != remote.position
}
