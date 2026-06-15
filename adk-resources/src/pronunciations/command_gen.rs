use crate::ids::stable_resource_id;
use crate::pronunciations::local::{
    PronunciationItem as LocalPronunciationItem, parse_pronunciations_content,
};
use crate::push_command;
use crate::push_command_inputs::{SimpleLifecycleCommands, json_bool, json_i32, json_str};
use crate::specs::PRONUNCIATIONS;
use adk_protobuf::Metadata;
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::pronunciations::{
    PronunciationsCreatePronunciation, PronunciationsDeletePronunciation,
    PronunciationsUpdatePronunciation,
};
use adk_types::ResourceMap;
use serde_json::Value as JsonValue;
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
    projection: &JsonValue,
    metadata: &Option<Metadata>,
) -> SimpleLifecycleCommands {
    let Some(content) = resources
        .get(PRONUNCIATIONS.file.file_path)
        .and_then(|resource| resource.payload.get("content"))
        .and_then(JsonValue::as_str)
    else {
        return SimpleLifecycleCommands::default();
    };
    let Some(local_items) = local_pronunciation_items(content) else {
        return SimpleLifecycleCommands::default();
    };
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

fn local_pronunciation_items(content: &str) -> Option<Vec<PronunciationItem>> {
    let file = parse_pronunciations_content(PRONUNCIATIONS.file.file_path, content).ok()?;
    Some(
        file.pronunciations
            .iter()
            .enumerate()
            .map(local_pronunciation_item)
            .collect(),
    )
}

fn local_pronunciation_item((idx, item): (usize, &LocalPronunciationItem)) -> PronunciationItem {
    PronunciationItem {
        id: String::new(),
        regex: item.regex().to_string(),
        replacement: item.replacement().to_string(),
        case_sensitive: item.case_sensitive(),
        language_code: item.language_code().to_string(),
        description: item.description().to_string(),
        position: idx as i32,
        name: item.name().to_string(),
    }
}

fn remote_pronunciation_items(projection: &JsonValue) -> Vec<PronunciationItem> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use adk_types::{Resource, ResourceMap};

    fn pronunciation_resource(content: &str) -> ResourceMap {
        let mut resources = ResourceMap::new();
        resources.insert(
            PRONUNCIATIONS.file.file_path.to_string(),
            Resource {
                resource_id: PRONUNCIATIONS.file.resource_id.to_string(),
                name: PRONUNCIATIONS.file.name.to_string(),
                file_path: PRONUNCIATIONS.file.file_path.to_string(),
                payload: serde_json::json!({ "content": content }),
            },
        );
        resources
    }

    fn projection_with_remote_pronunciation() -> JsonValue {
        serde_json::json!({
            "pronunciations": {
                "pronunciations": {
                    "ids": ["pn-greeting"],
                    "entities": {
                        "pn-greeting": {
                            "id": "pn-greeting",
                            "regex": "hello",
                            "replacement": "hullo",
                            "caseSensitive": false,
                            "languageCode": "en-GB",
                            "description": "Greeting",
                            "position": 0,
                            "name": "Greeting"
                        }
                    }
                }
            }
        })
    }

    #[test]
    fn local_pronunciation_items_use_typed_python_position_and_description_rules() {
        let content = r#"
pronunciations:
  - regex: first
    replacement: one
    description: "  trimmed  "
    position: 42
  - regex: second
    replacement: two
"#;

        let items = local_pronunciation_items(content).expect("pronunciations file should parse");

        assert_eq!(items[0].position, 0);
        assert_eq!(items[0].description, "trimmed");
        assert_eq!(items[1].position, 1);
    }

    #[test]
    fn parse_errors_do_not_delete_remote_pronunciations() {
        let content = r#"
pronunciations:
  - regex: ""
    replacement: hullo
"#;
        let resources = pronunciation_resource(content);
        let projection = projection_with_remote_pronunciation();

        let commands = pronunciation_lifecycle_commands(&resources, &projection, &None);

        assert!(commands.deletes.is_empty());
        assert!(commands.creates.is_empty());
        assert!(commands.updates.is_empty());
    }
}
