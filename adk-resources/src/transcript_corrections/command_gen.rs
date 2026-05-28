use crate::command_gen::local_file_helpers::{
    SimpleLifecycleCommands, json_str, resource_yaml, yaml_sequence,
};
use crate::ids::stable_resource_id;
use crate::specs::TRANSCRIPT_CORRECTIONS;
use crate::{push_command, yaml_str};
use adk_protobuf::Metadata;
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::transcript_corrections::{
    RegularExpression, TranscriptCorrection, TranscriptCorrectionsCreateTranscriptCorrections,
    TranscriptCorrectionsDeleteTranscriptCorrections, TranscriptCorrectionsUpdateData,
    TranscriptCorrectionsUpdateTranscriptCorrections,
};
use adk_types::ResourceMap;
use serde_json::{Value, json};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq)]
struct TranscriptItem {
    id: String,
    name: String,
    description: String,
    regular_expressions: Vec<RegularExpression>,
}

pub(crate) fn transcript_lifecycle_commands(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> SimpleLifecycleCommands {
    let Some(yaml) = resource_yaml(resources, TRANSCRIPT_CORRECTIONS.file.file_path) else {
        return SimpleLifecycleCommands::default();
    };
    let local_items = local_transcript_items(&yaml);
    let remote_items = remote_transcript_items(projection);
    let local_names = local_items
        .iter()
        .map(|item| item.name.clone())
        .collect::<HashSet<_>>();
    let remote_by_name = remote_items
        .iter()
        .map(|item| (item.name.clone(), item.clone()))
        .collect::<std::collections::HashMap<_, _>>();

    let mut commands = SimpleLifecycleCommands::default();
    for remote in &remote_items {
        if !local_names.contains(&remote.name) {
            push_command(
                &mut commands.deletes,
                metadata,
                "delete_transcript_corrections",
                CommandPayload::DeleteTranscriptCorrections(
                    TranscriptCorrectionsDeleteTranscriptCorrections {
                        transcript_corrections_id: remote.id.clone(),
                    },
                ),
            );
        }
    }
    let mut updated_corrections = Vec::new();
    for local in &local_items {
        match remote_by_name.get(&local.name) {
            Some(remote) => {
                let merged = transcript_item_with_remote_regex_ids(local, remote);
                if &merged != remote {
                    updated_corrections.push(transcript_correction_proto(&merged));
                }
            }
            None => {
                let id = stable_resource_id(
                    TRANSCRIPT_CORRECTIONS.id_prefix,
                    &local.name,
                    TRANSCRIPT_CORRECTIONS.file.file_path,
                );
                push_command(
                    &mut commands.creates,
                    metadata,
                    "create_transcript_corrections",
                    CommandPayload::CreateTranscriptCorrections(
                        TranscriptCorrectionsCreateTranscriptCorrections {
                            id: id.clone(),
                            name: local.name.clone(),
                            description: Some(local.description.clone()),
                            regular_expressions: transcript_regexes_with_ids(local, &id),
                        },
                    ),
                );
            }
        }
    }
    if !updated_corrections.is_empty() {
        push_command(
            &mut commands.updates,
            metadata,
            "update_transcript_corrections",
            CommandPayload::UpdateTranscriptCorrections(
                TranscriptCorrectionsUpdateTranscriptCorrections {
                    data: Some(TranscriptCorrectionsUpdateData {
                        corrections: updated_corrections,
                    }),
                },
            ),
        );
    }
    commands
}

fn local_transcript_items(yaml: &serde_yaml::Value) -> Vec<TranscriptItem> {
    yaml_sequence(yaml, TRANSCRIPT_CORRECTIONS.yaml_key)
        .into_iter()
        .filter_map(|item| {
            let name = yaml_str(item, "name");
            if name.is_empty() {
                return None;
            }
            Some(TranscriptItem {
                id: String::new(),
                name,
                description: yaml_str(item, "description"),
                regular_expressions: regexes_from_yaml(item),
            })
        })
        .collect()
}

fn remote_transcript_items(projection: &Value) -> Vec<TranscriptItem> {
    TRANSCRIPT_CORRECTIONS
        .entries(projection)
        .into_iter()
        .filter_map(|(id, value)| {
            let name = json_str(value, &["name"]);
            if name.is_empty() {
                return None;
            }
            Some(TranscriptItem {
                id,
                name,
                description: json_str(value, &["description"]),
                regular_expressions: regexes_from_projection(value),
            })
        })
        .collect()
}

fn regexes_from_yaml(item: &serde_yaml::Value) -> Vec<RegularExpression> {
    yaml_sequence(item, "regular_expressions")
        .into_iter()
        .map(|regex| RegularExpression {
            id: yaml_str(regex, "id"),
            regular_expression: yaml_str(regex, "regular_expression"),
            replacement: yaml_str(regex, "replacement"),
            replacement_type: yaml_str(regex, "replacement_type"),
        })
        .collect()
}

fn regexes_from_projection(item: &Value) -> Vec<RegularExpression> {
    item.get("regularExpressions")
        .or_else(|| item.get("regular_expressions"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|regex| RegularExpression {
            id: json_str(regex, &["id"]),
            regular_expression: json_str(regex, &["regularExpression", "regular_expression"]),
            replacement: json_str(regex, &["replacement"]),
            replacement_type: json_str(regex, &["replacementType", "replacement_type"]),
        })
        .collect()
}

fn transcript_item_with_remote_regex_ids(
    local: &TranscriptItem,
    remote: &TranscriptItem,
) -> TranscriptItem {
    let mut merged = local.clone();
    merged.id = remote.id.clone();
    for (idx, regex) in merged.regular_expressions.iter_mut().enumerate() {
        if regex.id.is_empty()
            && let Some(remote_id) = remote
                .regular_expressions
                .get(idx)
                .map(|regex| regex.id.clone())
                .filter(|id| !id.is_empty())
        {
            regex.id = remote_id;
        }
    }
    merged
}

fn transcript_regexes_with_ids(item: &TranscriptItem, id: &str) -> Vec<RegularExpression> {
    item.regular_expressions
        .iter()
        .enumerate()
        .map(|(idx, regex)| {
            let mut regex = regex.clone();
            if regex.id.is_empty() {
                regex.id = format!("{id}-REGEX-{idx}");
            }
            regex
        })
        .collect()
}

fn transcript_correction_proto(item: &TranscriptItem) -> TranscriptCorrection {
    TranscriptCorrection {
        id: item.id.clone(),
        name: item.name.clone(),
        description: item.description.clone(),
        regular_expressions: item.regular_expressions.clone(),
        created_by: String::new(),
        created_at: None,
        updated_by: String::new(),
        updated_at: None,
    }
}

pub(crate) fn regular_expression_json(regex: &RegularExpression) -> Value {
    json!({
        "id": regex.id,
        "regular_expression": regex.regular_expression,
        "replacement": regex.replacement,
        "replacement_type": regex.replacement_type,
    })
}

pub(crate) fn transcript_correction_json(correction: &TranscriptCorrection) -> Value {
    json!({
        "id": correction.id,
        "name": correction.name,
        "description": correction.description,
        "regular_expressions": correction.regular_expressions.iter().map(regular_expression_json).collect::<Vec<_>>(),
    })
}
