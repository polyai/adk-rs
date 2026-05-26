//! Push commands for singleton files.
//!
//! Singleton files represent one backend or configuration resource with their
//! own command semantics, such as role, personality, rules, channel settings,
//! ASR settings, safety filters, and experimental config.

mod settings;
mod summaries;

use super::CommandGroups;
use crate::{
    default_metadata_created_by, extract_entities_map, is_synthetic_local_resource_id,
    projection_to_resource_map, prompt_reference_maps_from_projection, push_command,
    replace_resource_names_with_ids,
    resource_specs::{AGENT_RULES_FILE, EXPERIMENTAL_CONFIG_FILE},
    rules_references_from_behaviour, rules_references_from_projection,
};
use adk_protobuf::Metadata;
use adk_protobuf::agent::RulesUpdateRules;
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::experimental_config::ExperimentalConfigUpdateConfig;
use adk_types::ResourceMap;
use prost_types::value::Kind;
use prost_types::{ListValue, Struct, Value as ProstValue};
use serde_json::Value;
use std::collections::BTreeMap;

pub(crate) fn singleton_command_groups(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> CommandGroups {
    let remote_resources = projection_to_resource_map(projection).unwrap_or_default();
    let mut groups = CommandGroups::default();

    append_rules_update(&mut groups.updates, resources, projection, metadata);
    append_experimental_config_update(&mut groups.updates, resources, projection, metadata);
    settings::append_agent_settings_updates(
        &mut groups.updates,
        resources,
        &remote_resources,
        metadata,
    );
    settings::append_channel_settings_updates(
        &mut groups.updates,
        resources,
        &remote_resources,
        metadata,
    );

    groups
}

fn append_rules_update(
    commands: &mut Vec<adk_protobuf::Command>,
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) {
    let Some(resource) = resources.get(AGENT_RULES_FILE.file_path) else {
        return;
    };
    let content = resource
        .payload
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let prompt_reference_maps = prompt_reference_maps_from_projection(projection);
    let normalized_content = replace_resource_names_with_ids(content, &prompt_reference_maps, None);
    let remote_behaviour = projection
        .pointer("/agentSettings/rules/behaviour")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if normalized_content == remote_behaviour {
        return;
    }
    push_command(
        commands,
        metadata,
        "update_rules",
        CommandPayload::UpdateRules(RulesUpdateRules {
            behaviour: Some(normalized_content.clone()),
            references: rules_references_from_behaviour(&normalized_content)
                .or_else(|| rules_references_from_projection(projection)),
        }),
    );
}

fn append_experimental_config_update(
    commands: &mut Vec<adk_protobuf::Command>,
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) {
    let Some(resource) = resources.get(EXPERIMENTAL_CONFIG_FILE.file_path) else {
        return;
    };
    let content = resource
        .payload
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if content.is_empty() {
        return;
    }
    let Ok(local_json) = serde_json::from_str::<Value>(content) else {
        return;
    };
    if remote_experimental_features(projection).as_ref() == Some(&local_json) {
        return;
    }
    let id = if !is_synthetic_local_resource_id(&resource.resource_id) {
        resource.resource_id.clone()
    } else {
        remote_experimental_config_id(projection).unwrap_or_else(|| "default".to_string())
    };
    push_command(
        commands,
        metadata,
        "experimental_config_update_config",
        CommandPayload::ExperimentalConfigUpdateConfig(ExperimentalConfigUpdateConfig {
            id,
            features: json_to_prost_struct(&local_json),
            updated_at: None,
            updated_by: sdk_user(metadata),
        }),
    );
}

fn remote_experimental_features(projection: &Value) -> Option<Value> {
    Some(
        projection
            .get("experimentalConfig")?
            .get("experimentalConfigs")?
            .get("entities")?
            .get("default")?
            .get("features")?
            .clone(),
    )
}

fn remote_experimental_config_id(projection: &Value) -> Option<String> {
    extract_entities_map(
        projection,
        &["experimentalConfig", "experimentalConfigs", "entities"],
    )
    .keys()
    .next()
    .cloned()
}

fn sdk_user(metadata: &Option<Metadata>) -> String {
    metadata
        .as_ref()
        .map(|m| m.created_by.trim())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(default_metadata_created_by)
}

fn json_to_prost_struct(value: &Value) -> Option<Struct> {
    let object = value.as_object()?;
    let mut fields = BTreeMap::new();
    for (key, value) in object {
        fields.insert(key.clone(), json_to_prost_value(value));
    }
    Some(Struct { fields })
}

fn json_to_prost_value(value: &Value) -> ProstValue {
    match value {
        Value::Null => ProstValue {
            kind: Some(Kind::NullValue(0)),
        },
        Value::Bool(value) => ProstValue {
            kind: Some(Kind::BoolValue(*value)),
        },
        Value::Number(value) => ProstValue {
            kind: Some(Kind::NumberValue(value.as_f64().unwrap_or(0.0))),
        },
        Value::String(value) => ProstValue {
            kind: Some(Kind::StringValue(value.clone())),
        },
        Value::Array(values) => ProstValue {
            kind: Some(Kind::ListValue(ListValue {
                values: values.iter().map(json_to_prost_value).collect(),
            })),
        },
        Value::Object(object) => {
            let mut fields = BTreeMap::new();
            for (key, value) in object {
                fields.insert(key.clone(), json_to_prost_value(value));
            }
            ProstValue {
                kind: Some(Kind::StructValue(Struct { fields })),
            }
        }
    }
}

pub(crate) fn payload_json_summary(payload: &CommandPayload) -> Option<(&'static str, Value)> {
    summaries::payload_json_summary(payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use adk_types::Resource;
    use indexmap::IndexMap;

    fn map_with(resources: Vec<(String, Resource)>) -> ResourceMap {
        let mut map: ResourceMap = IndexMap::new();
        for (path, resource) in resources {
            map.insert(path, resource);
        }
        map
    }

    fn flatten(groups: CommandGroups) -> Vec<adk_protobuf::Command> {
        groups
            .deletes
            .into_iter()
            .chain(groups.creates)
            .chain(groups.updates)
            .chain(groups.post_updates)
            .collect()
    }

    #[test]
    fn experimental_config_singleton_emits_update() {
        let resources = map_with(vec![(
            EXPERIMENTAL_CONFIG_FILE.file_path.into(),
            Resource {
                resource_id: "default".into(),
                name: EXPERIMENTAL_CONFIG_FILE.name.into(),
                file_path: EXPERIMENTAL_CONFIG_FILE.file_path.into(),
                payload: serde_json::json!({
                    "content": r#"{ "flag_test": true }"#
                }),
            },
        )]);
        let commands = flatten(singleton_command_groups(
            &resources,
            &serde_json::json!({}),
            &None,
        ));
        let command = commands
            .iter()
            .find(|command| command.r#type == "experimental_config_update_config")
            .expect("experimental config update command");
        match &command.payload {
            Some(CommandPayload::ExperimentalConfigUpdateConfig(payload)) => {
                assert_eq!(payload.id, "default");
            }
            _ => panic!("unexpected payload variant for experimental config update command"),
        }
    }
}
