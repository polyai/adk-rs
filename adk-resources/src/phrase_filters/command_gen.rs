//! Push commands for phrase filter aggregate files.
//!
//! This covers phrase filters (stop keywords), and mirrors Python
//! `poly.resources.*` command type strings.
//!
//! **Execution ordering:** Python `SyncClientHandler.queue_resources` walks deletes (respecting
//! `PRIORITY_DELETE_TYPES`), then creates (`PRIORITY_CREATE_TYPES`), then updates
//! (`PRIORITY_UPDATE_TYPES`).
//! This module emits per-family command groups; `build_push_commands` applies the global
//! delete/create/update ordering across all resource-family modules.

use crate::ids::stable_resource_id;
use crate::phrase_filters::local::{
    PHRASE_FILTERS_FILE_PATH, PhraseFilterItem as LocalPhraseFilterItem,
    parse_phrase_filters_content,
};
use crate::{extract_entities_map, is_synthetic_local_resource_id, push_command};
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::stop_keywords::{
    StopKeywordCreate, StopKeywordDelete, StopKeywordReferences, StopKeywordUpdate,
};
use adk_protobuf::{Command, Metadata};
use adk_types::ResourceMap;
use serde_json::{self, Value as JsonValue};
use std::collections::{HashMap, HashSet};

use crate::push_commands::CommandGroups;

fn remote_phrase_filters(projection: &JsonValue) -> HashMap<String, String> {
    let entities = extract_entities_map(projection, &["stopKeywords", "filters", "entities"]);
    let mut m = HashMap::new();
    for (id, v) in entities {
        let title = v
            .get("title")
            .and_then(JsonValue::as_str)
            .unwrap_or(&id)
            .to_string();
        m.insert(title, id);
    }
    m
}

fn phrase_refs(function_id: Option<&str>) -> Option<StopKeywordReferences> {
    let mut global_functions = HashMap::new();
    if let Some(fid) = function_id.filter(|s| !s.is_empty()) {
        global_functions.insert(fid.to_string(), true);
    }
    if global_functions.is_empty() {
        return None;
    }
    Some(StopKeywordReferences { global_functions })
}

struct PhraseFilterCommandBuilder<'a> {
    projection: &'a JsonValue,
    remote: &'a HashMap<String, String>,
    metadata: &'a Option<Metadata>,
    local_titles: &'a mut HashSet<String>,
    creates: &'a mut Vec<Command>,
    updates: &'a mut Vec<Command>,
}

impl PhraseFilterCommandBuilder<'_> {
    fn append_item(&mut self, item: &LocalPhraseFilterItem, resource_id: &str) {
        let title = item.name().to_string();
        self.local_titles.insert(title.clone());
        let id = self
            .remote
            .get(&title)
            .cloned()
            .or_else(|| {
                (!is_synthetic_local_resource_id(resource_id)).then_some(resource_id.to_string())
            })
            .unwrap_or_else(|| {
                stable_resource_id("PHRASE_FILTERING", &title, PHRASE_FILTERS_FILE_PATH)
            });
        let description = item.description().to_string();
        let say_phrase = item.say_phrase();
        let language_code = item.language_code().to_string();
        let regular_expressions = item.regular_expressions().to_vec();
        let references = phrase_refs(item.function());

        if self.remote.contains_key(&title) {
            if let Some(remote) = self.remote.get(&title).and_then(|id| {
                extract_entities_map(self.projection, &["stopKeywords", "filters", "entities"])
                    .get(id)
                    .cloned()
            }) && phrase_filter_matches_remote(item, &remote)
            {
                return;
            }
            push_command(
                self.updates,
                self.metadata,
                "stop_keywords_update",
                CommandPayload::StopKeywordsUpdate(StopKeywordUpdate {
                    id: id.clone(),
                    title: Some(title.clone()),
                    description: Some(description),
                    regular_expressions,
                    say_phrase: Some(say_phrase),
                    references: references.clone(),
                    language_code: Some(language_code),
                }),
            );
        } else {
            push_command(
                self.creates,
                self.metadata,
                "stop_keywords_create",
                CommandPayload::StopKeywordsCreate(StopKeywordCreate {
                    id: id.clone(),
                    title,
                    description,
                    regular_expressions,
                    say_phrase,
                    references,
                    language_code,
                }),
            );
        }
    }
}

fn phrase_filter_matches_remote(local: &LocalPhraseFilterItem, remote: &JsonValue) -> bool {
    local.name()
        == remote
            .get("title")
            .and_then(JsonValue::as_str)
            .unwrap_or("")
        && local.description()
            == remote
                .get("description")
                .and_then(JsonValue::as_str)
                .unwrap_or("")
        && local.say_phrase()
            == remote
                .get("sayPhrase")
                .and_then(JsonValue::as_bool)
                .unwrap_or(false)
        && local.language_code()
            == remote
                .get("languageCode")
                .and_then(JsonValue::as_str)
                .unwrap_or("")
        && local.regular_expressions()
            == remote
                .get("regularExpressions")
                .and_then(JsonValue::as_array)
                .into_iter()
                .flatten()
                .filter_map(JsonValue::as_str)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
}

/// Builds push commands for interaction resources stored in aggregate files.
///
/// Each resource type can be represented either as a whole local YAML file or as
/// per-item logical resources from the status snapshot, so this function accepts
/// both forms, normalizes them into local item sets, and compares those sets
/// with the Agent Studio projection.
///
pub(crate) fn phrase_filter_command_groups(
    resources: &ResourceMap,
    projection: &JsonValue,
    metadata: &Option<Metadata>,
) -> CommandGroups {
    let mut sk_del = Vec::new();
    let mut sk_create = Vec::new();
    let mut sk_update = Vec::new();
    let rpf = remote_phrase_filters(projection);

    let mut local_pf_titles = HashSet::new();
    let local_phrase_filters = local_phrase_filter_resources(resources);

    {
        let mut phrase_filter_builder = PhraseFilterCommandBuilder {
            projection,
            remote: &rpf,
            metadata,
            local_titles: &mut local_pf_titles,
            creates: &mut sk_create,
            updates: &mut sk_update,
        };

        for local in &local_phrase_filters {
            phrase_filter_builder.append_item(&local.item, &local.resource_id);
        }
    }

    for (title, id) in &rpf {
        if !local_pf_titles.contains(title) {
            push_command(
                &mut sk_del,
                metadata,
                "stop_keywords_delete",
                CommandPayload::StopKeywordsDelete(StopKeywordDelete { id: id.clone() }),
            );
        }
    }

    let mut groups = CommandGroups::default();
    groups.deletes.extend(sk_del);
    groups.creates.extend(sk_create);
    groups.updates.extend(sk_update);
    groups
}

struct LocalPhraseFilterResource {
    resource_id: String,
    item: LocalPhraseFilterItem,
}

fn local_phrase_filter_resources(resources: &ResourceMap) -> Vec<LocalPhraseFilterResource> {
    let mut phrase_filters = Vec::new();
    for resource in resources.values() {
        let path = resource.file_path.as_str();
        let content = resource
            .payload
            .get("content")
            .and_then(JsonValue::as_str)
            .unwrap_or_default();
        let Ok(items) = parse_phrase_filters_content(path, content) else {
            continue;
        };
        let resource_id = if path == PHRASE_FILTERS_FILE_PATH {
            "local"
        } else {
            resource.resource_id.as_str()
        };
        phrase_filters.extend(items.into_iter().map(|item| LocalPhraseFilterResource {
            resource_id: resource_id.to_string(),
            item,
        }));
    }
    phrase_filters
}

pub(crate) fn payload_json_summary(payload: &CommandPayload) -> Option<(&'static str, JsonValue)> {
    match payload {
        CommandPayload::StopKeywordsDelete(delete) => Some((
            "stop_keywords_delete",
            serde_json::json!({
                "id": delete.id,
            }),
        )),
        CommandPayload::StopKeywordsCreate(create) => {
            let mut value = serde_json::Map::new();
            value.insert("id".to_string(), JsonValue::String(create.id.clone()));
            value.insert("title".to_string(), JsonValue::String(create.title.clone()));
            value.insert(
                "description".to_string(),
                JsonValue::String(create.description.clone()),
            );
            value.insert(
                "regular_expressions".to_string(),
                serde_json::json!(create.regular_expressions),
            );
            if create.say_phrase {
                value.insert("say_phrase".to_string(), JsonValue::Bool(true));
            }
            value.insert(
                "references".to_string(),
                stop_keyword_references_json(create.references.as_ref()),
            );
            value.insert(
                "language_code".to_string(),
                JsonValue::String(create.language_code.clone()),
            );
            Some(("stop_keywords_create", JsonValue::Object(value)))
        }
        CommandPayload::StopKeywordsUpdate(update) => {
            let mut value = serde_json::Map::new();
            value.insert("id".to_string(), JsonValue::String(update.id.clone()));
            value.insert(
                "title".to_string(),
                JsonValue::String(update.title.clone().unwrap_or_default()),
            );
            value.insert(
                "description".to_string(),
                JsonValue::String(update.description.clone().unwrap_or_default()),
            );
            value.insert(
                "regular_expressions".to_string(),
                serde_json::json!(update.regular_expressions),
            );
            if let Some(say_phrase) = update.say_phrase {
                value.insert("say_phrase".to_string(), JsonValue::Bool(say_phrase));
            }
            value.insert(
                "references".to_string(),
                stop_keyword_references_json(update.references.as_ref()),
            );
            value.insert(
                "language_code".to_string(),
                JsonValue::String(update.language_code.clone().unwrap_or_default()),
            );
            Some(("stop_keywords_update", JsonValue::Object(value)))
        }
        _ => None,
    }
}

fn stop_keyword_references_json(references: Option<&StopKeywordReferences>) -> JsonValue {
    let Some(references) = references else {
        return serde_json::json!({});
    };
    if references.global_functions.is_empty() {
        serde_json::json!({})
    } else {
        serde_json::json!({
            "global_functions": references.global_functions,
        })
    }
}

#[cfg(test)]
#[path = "command_gen_tests.rs"]
mod command_gen_tests;
