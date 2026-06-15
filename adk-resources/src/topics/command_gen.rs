//! Push commands for knowledge-base topic resources.

use serde_json::Value as JsonValue;

use crate::ids::stable_resource_id;
use crate::push_commands::CommandGroups;
use crate::topics::local::{LocalTopic, deserialize_topic_content, topic_references};
use crate::{
    extract_entities_map, is_synthetic_local_resource_id, prompt_reference_maps_from_projection,
    push_command, replace_resource_names_with_ids,
};
use adk_protobuf::Metadata;
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::knowledge_base::{
    KnowledgeBaseCreateTopic, KnowledgeBaseDeleteTopic, KnowledgeBaseUpdateTopic,
};
use adk_types::ResourceMap;
use std::collections::{HashMap, HashSet};

#[cfg(test)]
#[path = "command_gen_tests.rs"]
mod command_gen_tests;

/// Builds create/update/delete commands for knowledge-base topics.
///
/// Local topic YAML stores readable names and prompt references, while Agent
/// Studio commands expect stable topic IDs and reference IDs. This function
/// resolves the remote topic set from the projection, translates local reference
/// names back to IDs, emits creates or updates for changed local topics, and
/// deletes remote topics that are no longer present on disk.
///
/// The returned commands are grouped into the standard push phases so topic
/// lifecycle changes compose predictably with the other command generators.
pub(crate) fn topic_resource_command_groups(
    resources: &ResourceMap,
    projection: &JsonValue,
    metadata: &Option<Metadata>,
) -> CommandGroups {
    let prompt_reference_maps = prompt_reference_maps_from_projection(projection);
    let remote_topics = topic_entries(projection)
        .into_iter()
        .map(|(id, topic)| {
            (
                topic
                    .get("name")
                    .and_then(JsonValue::as_str)
                    .unwrap_or(id.as_str())
                    .to_string(),
                (id, topic),
            )
        })
        .collect::<HashMap<_, _>>();
    let local_topics = local_topic_resources(resources);
    let mut local_topic_names = HashSet::new();
    let mut groups = CommandGroups::default();

    for local in &local_topics.topics {
        let name = local.topic.name().to_string();
        local_topic_names.insert(name.clone());
        let remote_topic = remote_topics.get(&name);
        let id = local_topic_id(&local.resource_id, &local.path, &name, remote_topic);
        let actions =
            replace_resource_names_with_ids(local.topic.actions(), &prompt_reference_maps, None);
        let text =
            replace_resource_names_with_ids(local.topic.content(), &prompt_reference_maps, None);
        let references = topic_references(&actions, &text);

        if let Some((_, remote_topic)) = remote_topic {
            if topic_yaml_matches_projection(&local.topic, &actions, &text, remote_topic) {
                continue;
            }
            push_command(
                &mut groups.updates,
                metadata,
                "update_topic",
                CommandPayload::UpdateTopic(KnowledgeBaseUpdateTopic {
                    id: id.clone(),
                    name: Some(name.clone()),
                    content: Some(text),
                    actions: Some(actions),
                    example_queries: Some(local.topic.example_queries_proto()),
                    references: Some(references),
                    is_active: Some(local.topic.enabled()),
                    variant_id: None,
                }),
            );
        } else {
            push_command(
                &mut groups.creates,
                metadata,
                "create_topic",
                CommandPayload::CreateTopic(KnowledgeBaseCreateTopic {
                    id: id.clone(),
                    name: name.clone(),
                    content: text,
                    actions,
                    example_queries: Some(local.topic.example_queries_proto()),
                    references: Some(references),
                    is_active: Some(local.topic.enabled()),
                    variant_id: None,
                }),
            );
        }
    }

    if !local_topics.had_parse_error {
        for (name, (id, _)) in remote_topics {
            if !local_topic_names.contains(&name) {
                push_command(
                    &mut groups.deletes,
                    metadata,
                    "delete_topic",
                    CommandPayload::DeleteTopic(KnowledgeBaseDeleteTopic { id }),
                );
            }
        }
    }

    groups
}

struct LocalTopicResources {
    topics: Vec<LocalTopicResource>,
    had_parse_error: bool,
}

struct LocalTopicResource {
    path: String,
    resource_id: String,
    topic: LocalTopic,
}

fn local_topic_resources(resources: &ResourceMap) -> LocalTopicResources {
    let mut topics = Vec::new();
    let mut had_parse_error = false;
    for resource in resources.values() {
        let path = resource.file_path.as_str();
        let content = resource
            .payload
            .get("content")
            .and_then(JsonValue::as_str)
            .unwrap_or_default();
        let topic = match deserialize_topic_content(path, content) {
            Ok(Some(topic)) => topic,
            Ok(None) => continue,
            Err(_) => {
                had_parse_error = true;
                continue;
            }
        };
        topics.push(LocalTopicResource {
            path: path.to_string(),
            resource_id: resource.resource_id.clone(),
            topic,
        });
    }
    LocalTopicResources {
        topics,
        had_parse_error,
    }
}

fn local_topic_id(
    resource_id: &str,
    path: &str,
    name: &str,
    remote_topic: Option<&(String, JsonValue)>,
) -> String {
    remote_topic
        .map(|(id, _)| id.clone())
        .or_else(|| {
            (!is_synthetic_local_resource_id(resource_id)).then_some(resource_id.to_string())
        })
        .unwrap_or_else(|| stable_resource_id("TOPICS", name, path))
}

pub(crate) fn topic_entries(projection: &JsonValue) -> HashMap<String, JsonValue> {
    extract_entities_map(projection, &["knowledgeBase", "topics", "entities"])
}

fn topic_yaml_matches_projection(
    local: &LocalTopic,
    actions: &str,
    content: &str,
    topic: &JsonValue,
) -> bool {
    let remote_name = topic
        .get("name")
        .and_then(JsonValue::as_str)
        .unwrap_or(local.name());
    let remote_enabled = topic
        .get("isActive")
        .and_then(JsonValue::as_bool)
        .unwrap_or(true);
    let remote_actions = topic
        .get("actions")
        .and_then(JsonValue::as_str)
        .unwrap_or("");
    let remote_content = topic
        .get("content")
        .and_then(JsonValue::as_str)
        .unwrap_or("");
    let remote_queries = topic
        .get("exampleQueries")
        .and_then(JsonValue::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    item.get("query")
                        .and_then(JsonValue::as_str)
                        .map(ToString::to_string)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    remote_name == local.name()
        && remote_enabled == local.enabled()
        && remote_actions == actions
        && remote_content == content
        && remote_queries == local.example_queries()
}
