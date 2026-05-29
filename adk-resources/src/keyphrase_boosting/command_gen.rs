use crate::ids::stable_resource_id;
use crate::push_command_inputs::{SimpleLifecycleCommands, json_str, resource_yaml, yaml_sequence};
use crate::specs::KEYPHRASE_BOOSTING;
use crate::{push_command, yaml_str};
use adk_protobuf::Metadata;
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::keyphrase_boosting::{
    KeyphraseBoostingCreateKeyphrase, KeyphraseBoostingDeleteKeyphrase,
    KeyphraseBoostingUpdateKeyphrase,
};
use adk_types::ResourceMap;
use serde_json::Value as JsonValue;
use serde_yaml_ng::Value as YamlValue;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq)]
struct KeyphraseItem {
    id: String,
    keyphrase: String,
    level: String,
}

pub(crate) fn keyphrase_lifecycle_commands(
    resources: &ResourceMap,
    projection: &JsonValue,
    metadata: &Option<Metadata>,
) -> SimpleLifecycleCommands {
    let Some(yaml) = resource_yaml(resources, KEYPHRASE_BOOSTING.file.file_path) else {
        return SimpleLifecycleCommands::default();
    };
    let local_items = local_keyphrase_items(&yaml);
    let remote_items = remote_keyphrase_items(projection);
    let local_keyphrases = local_items
        .iter()
        .map(|item| item.keyphrase.clone())
        .collect::<HashSet<_>>();
    let remote_by_keyphrase = remote_items
        .iter()
        .map(|item| (item.keyphrase.clone(), item.clone()))
        .collect::<HashMap<_, _>>();

    let mut commands = SimpleLifecycleCommands::default();
    for remote in &remote_items {
        if !local_keyphrases.contains(&remote.keyphrase) {
            push_command(
                &mut commands.deletes,
                metadata,
                "delete_keyphrase_boosting",
                CommandPayload::DeleteKeyphraseBoosting(KeyphraseBoostingDeleteKeyphrase {
                    id: remote.id.clone(),
                }),
            );
        }
    }
    for local in &local_items {
        match remote_by_keyphrase.get(&local.keyphrase) {
            Some(remote) if local.level != remote.level => push_command(
                &mut commands.updates,
                metadata,
                "update_keyphrase_boosting",
                CommandPayload::UpdateKeyphraseBoosting(KeyphraseBoostingUpdateKeyphrase {
                    id: remote.id.clone(),
                    keyphrase: Some(local.keyphrase.clone()),
                    level: Some(local.level.clone()),
                }),
            ),
            Some(_) => {}
            None => {
                let id = stable_resource_id(
                    KEYPHRASE_BOOSTING.id_prefix,
                    &local.keyphrase,
                    KEYPHRASE_BOOSTING.file.file_path,
                );
                push_command(
                    &mut commands.creates,
                    metadata,
                    "create_keyphrase_boosting",
                    CommandPayload::CreateKeyphraseBoosting(KeyphraseBoostingCreateKeyphrase {
                        id,
                        keyphrase: local.keyphrase.clone(),
                        level: local.level.clone(),
                    }),
                );
            }
        }
    }
    commands
}

fn local_keyphrase_items(yaml: &YamlValue) -> Vec<KeyphraseItem> {
    yaml_sequence(yaml, KEYPHRASE_BOOSTING.yaml_key)
        .into_iter()
        .filter_map(|item| {
            let keyphrase = yaml_str(item, "keyphrase");
            if keyphrase.is_empty() {
                return None;
            }
            Some(KeyphraseItem {
                id: String::new(),
                keyphrase,
                level: yaml_str(item, "level"),
            })
        })
        .collect()
}

fn remote_keyphrase_items(projection: &JsonValue) -> Vec<KeyphraseItem> {
    KEYPHRASE_BOOSTING
        .entries(projection)
        .into_iter()
        .filter_map(|(id, value)| {
            let keyphrase = json_str(value, &["keyphrase"]);
            if keyphrase.is_empty() {
                return None;
            }
            Some(KeyphraseItem {
                id,
                keyphrase,
                level: json_str(value, &["level"]),
            })
        })
        .collect()
}
