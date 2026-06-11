//! Push commands for knowledge-base topic resources.

use serde_json::Value as JsonValue;
use serde_yaml_ng::{Value as YamlValue, from_str};

use crate::ids::stable_resource_id;
use crate::push_commands::CommandGroups;
use crate::{
    extract_entities_map, is_synthetic_local_resource_id, prompt_reference_maps_from_projection,
    push_command, replace_resource_names_with_ids,
};
use adk_protobuf::Metadata;
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::knowledge_base::{
    ExampleQueries, KnowledgeBaseCreateTopic, KnowledgeBaseDeleteTopic, KnowledgeBaseUpdateTopic,
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
    let mut local_topic_names = HashSet::new();
    let mut groups = CommandGroups::default();

    for resource in resources.values() {
        let path = resource.file_path.as_str();
        if !path.starts_with("topics/") || !path.ends_with(".yaml") {
            continue;
        }
        let content = resource
            .payload
            .get("content")
            .and_then(JsonValue::as_str)
            .unwrap_or_default();
        let Ok(yaml) = from_str::<YamlValue>(content) else {
            continue;
        };
        let name = yaml
            .get("name")
            .and_then(YamlValue::as_str)
            .unwrap_or(&resource.name)
            .to_string();
        local_topic_names.insert(name.clone());
        let remote_topic = remote_topics.get(&name);
        let id = remote_topic
            .map(|(id, _)| id.clone())
            .or_else(|| {
                (!is_synthetic_local_resource_id(&resource.resource_id))
                    .then_some(resource.resource_id.clone())
            })
            .unwrap_or_else(|| stable_resource_id("TOPICS", &name, path));
        let actions = yaml
            .get("actions")
            .and_then(YamlValue::as_str)
            .map(|value| replace_resource_names_with_ids(value, &prompt_reference_maps, None))
            .unwrap_or_default();
        let text = yaml
            .get("content")
            .and_then(YamlValue::as_str)
            .map(|value| replace_resource_names_with_ids(value, &prompt_reference_maps, None))
            .unwrap_or_default();
        let enabled = yaml
            .get("enabled")
            .and_then(YamlValue::as_bool)
            .unwrap_or(true);
        let example_queries = yaml
            .get("example_queries")
            .and_then(YamlValue::as_sequence)
            .map(|seq| {
                seq.iter()
                    .filter_map(YamlValue::as_str)
                    .map(ToString::to_string)
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();

        if let Some((_, remote_topic)) = remote_topic {
            if topic_yaml_matches_projection(
                &name,
                enabled,
                &actions,
                &text,
                &example_queries,
                remote_topic,
            ) {
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
                    example_queries: Some(ExampleQueries {
                        queries: example_queries,
                    }),
                    references: None,
                    is_active: Some(enabled),
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
                    example_queries: Some(ExampleQueries {
                        queries: example_queries,
                    }),
                    references: None,
                    is_active: Some(enabled),
                    variant_id: None,
                }),
            );
        }
    }

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

    groups
}

pub(crate) fn topic_entries(projection: &JsonValue) -> HashMap<String, JsonValue> {
    extract_entities_map(projection, &["knowledgeBase", "topics", "entities"])
}

fn topic_yaml_matches_projection(
    name: &str,
    enabled: bool,
    actions: &str,
    content: &str,
    example_queries: &[String],
    topic: &JsonValue,
) -> bool {
    let remote_name = topic
        .get("name")
        .and_then(JsonValue::as_str)
        .unwrap_or(name);
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
    remote_name == name
        && remote_enabled == enabled
        && remote_actions == actions
        && remote_content == content
        && remote_queries == example_queries
}
