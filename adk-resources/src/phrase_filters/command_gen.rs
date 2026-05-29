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

use serde_json::{self, Value as JsonValue};
use serde_yaml_ng::{Value as YamlValue, from_str};

use crate::ids::stable_resource_id;
use crate::{extract_entities_map, is_synthetic_local_resource_id, push_command};
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::stop_keywords::{
    StopKeywordCreate, StopKeywordDelete, StopKeywordReferences, StopKeywordUpdate,
};
use adk_protobuf::{Command, Metadata};
use adk_types::ResourceMap;
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

fn yaml_str(y: &YamlValue, key: &str) -> String {
    y.get(key)
        .and_then(YamlValue::as_str)
        .unwrap_or("")
        .to_string()
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

fn phrase_refs_from_yaml(yaml: &YamlValue) -> Option<StopKeywordReferences> {
    let mut global_functions = HashMap::new();
    if let Some(fid) = yaml.get("function").and_then(YamlValue::as_str)
        && !fid.trim().is_empty()
    {
        global_functions.insert(fid.to_string(), true);
    }
    if let Some(refs) = yaml.get("references").or_else(|| yaml.get("refs"))
        && let Some(gf) = refs
            .get("global_functions")
            .or_else(|| refs.get("globalFunctions"))
    {
        if let Some(arr) = gf.as_sequence() {
            for item in arr {
                if let Some(fid) = item.as_str()
                    && !fid.trim().is_empty()
                {
                    global_functions.insert(fid.to_string(), true);
                }
            }
        } else if let Some(map) = gf.as_mapping() {
            for (k, v) in map {
                if let Some(fid) = k.as_str()
                    && !fid.trim().is_empty()
                {
                    global_functions.insert(fid.to_string(), v.as_bool().unwrap_or(true));
                }
            }
        }
    }
    if global_functions.is_empty() {
        None
    } else {
        Some(StopKeywordReferences { global_functions })
    }
}

struct PhraseFilterItemQueue<'a> {
    projection: &'a JsonValue,
    remote: &'a HashMap<String, String>,
    metadata: &'a Option<Metadata>,
    local_titles: &'a mut HashSet<String>,
    creates: &'a mut Vec<Command>,
    updates: &'a mut Vec<Command>,
}

impl PhraseFilterItemQueue<'_> {
    fn queue(&mut self, yaml: &YamlValue, resource_id: &str) {
        let title = yaml_str(yaml, "name");
        if title.is_empty() {
            return;
        }
        self.local_titles.insert(title.clone());
        let id = self
            .remote
            .get(&title)
            .cloned()
            .or_else(|| {
                (!is_synthetic_local_resource_id(resource_id)).then_some(resource_id.to_string())
            })
            .unwrap_or_else(|| {
                stable_resource_id(
                    "PHRASE_FILTERING",
                    &title,
                    "voice/response_control/phrase_filtering.yaml",
                )
            });
        let description = yaml_str(yaml, "description");
        let say_phrase = yaml
            .get("say_phrase")
            .or_else(|| yaml.get("sayPhrase"))
            .and_then(YamlValue::as_bool)
            .unwrap_or(false);
        let language_code = yaml_str(yaml, "language_code");
        let language_code = if language_code.is_empty() {
            yaml_str(yaml, "languageCode")
        } else {
            language_code
        };
        let regular_expressions: Vec<String> = yaml
            .get("regular_expressions")
            .or_else(|| yaml.get("regularExpressions"))
            .and_then(YamlValue::as_sequence)
            .map(|seq| {
                seq.iter()
                    .filter_map(YamlValue::as_str)
                    .map(ToString::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let references = phrase_refs_from_yaml(yaml).or_else(|| {
            let function_id = yaml
                .get("function")
                .and_then(YamlValue::as_str)
                .map(ToString::to_string);
            phrase_refs(function_id.as_deref())
        });

        if self.remote.contains_key(&title) {
            if let Some(remote) = self.remote.get(&title).and_then(|id| {
                extract_entities_map(self.projection, &["stopKeywords", "filters", "entities"])
                    .get(id)
                    .cloned()
            }) && phrase_filter_matches_remote(yaml, &remote)
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

fn phrase_filter_matches_remote(local: &YamlValue, remote: &JsonValue) -> bool {
    let local_language_code = non_empty(
        yaml_str(local, "language_code"),
        yaml_str(local, "languageCode"),
    );
    yaml_str(local, "name")
        == remote
            .get("title")
            .and_then(JsonValue::as_str)
            .unwrap_or("")
        && yaml_str(local, "description")
            == remote
                .get("description")
                .and_then(JsonValue::as_str)
                .unwrap_or("")
        && yaml_bool(
            local.get("say_phrase").or_else(|| local.get("sayPhrase")),
            false,
        ) == remote
            .get("sayPhrase")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false)
        && local_language_code
            == remote
                .get("languageCode")
                .and_then(JsonValue::as_str)
                .unwrap_or("")
        && yaml_string_list(
            local
                .get("regular_expressions")
                .or_else(|| local.get("regularExpressions")),
        ) == remote
            .get("regularExpressions")
            .and_then(JsonValue::as_array)
            .into_iter()
            .flatten()
            .filter_map(JsonValue::as_str)
            .map(ToString::to_string)
            .collect::<Vec<_>>()
}

fn non_empty(left: String, right: String) -> String {
    if left.is_empty() { right } else { left }
}

fn yaml_bool(value: Option<&YamlValue>, default: bool) -> bool {
    value.and_then(YamlValue::as_bool).unwrap_or(default)
}

fn yaml_string_list(value: Option<&YamlValue>) -> Vec<String> {
    value
        .and_then(YamlValue::as_sequence)
        .map(|seq| {
            seq.iter()
                .filter_map(YamlValue::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
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

    {
        let mut phrase_filter_queue = PhraseFilterItemQueue {
            projection,
            remote: &rpf,
            metadata,
            local_titles: &mut local_pf_titles,
            creates: &mut sk_create,
            updates: &mut sk_update,
        };

        for resource in resources.values() {
            let path = resource.file_path.as_str();
            let content = resource
                .payload
                .get("content")
                .and_then(JsonValue::as_str)
                .unwrap_or_default();

            if path == "voice/response_control/phrase_filtering.yaml" {
                if let Ok(yaml) = from_str::<YamlValue>(content)
                    && let Some(items) = yaml
                        .get("phrase_filtering")
                        .and_then(YamlValue::as_sequence)
                {
                    for item in items {
                        phrase_filter_queue.queue(item, "local");
                    }
                }
                continue;
            }

            if path.starts_with("voice/response_control/phrase_filtering.yaml/phrase_filtering/") {
                if let Ok(yaml) = from_str::<YamlValue>(content) {
                    phrase_filter_queue.queue(&yaml, &resource.resource_id);
                }
                continue;
            }
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
