//! Push commands for aggregate files.
//!
//! Aggregate files contain a list or map of peer backend resources in one local
//! file. Examples include entities, variants, API integrations, handoffs, SMS
//! templates, stop-keyword phrase filters, pronunciations, keyphrase boosting,
//! and transcript corrections.

mod interactions;
mod summaries;

use super::CommandGroups;
use adk_protobuf::Metadata;
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::entities::{self, EntityCreate, EntityDelete, EntityUpdate};
use adk_types::ResourceMap;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

use crate::ids::stable_resource_id;
use crate::specs::{ENTITIES_FILE, ENTITY_ID_PREFIX};
use crate::{
    build_entity_create_config, build_entity_update_config, entity_entries,
    is_synthetic_local_resource_id, push_command, to_camel_case,
};

pub(crate) fn aggregate_command_groups(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> CommandGroups {
    let mut groups = entity_aggregate_command_groups(resources, projection, metadata);
    groups.append(interactions::interaction_aggregate_command_groups(
        resources, projection, metadata,
    ));

    let variant_lifecycle: crate::variants::VariantLifecycleCommands =
        crate::variants::variant_lifecycle_commands(resources, projection, metadata);
    let api_lifecycle: crate::api_integrations::ApiIntegrationLifecycleCommands =
        crate::api_integrations::api_integration_lifecycle_commands(
            resources, projection, metadata,
        );
    let keyphrase_lifecycle =
        crate::keyphrase_boosting::keyphrase_lifecycle_commands(resources, projection, metadata);
    let transcript_lifecycle = crate::transcript_corrections::transcript_lifecycle_commands(
        resources, projection, metadata,
    );
    let pronunciation_lifecycle =
        crate::pronunciations::pronunciation_lifecycle_commands(resources, projection, metadata);

    groups.deletes.extend(variant_lifecycle.variant_deletes);
    groups.deletes.extend(api_lifecycle.integration_deletes);
    groups.deletes.extend(keyphrase_lifecycle.deletes);
    groups.deletes.extend(variant_lifecycle.attribute_deletes);
    groups.deletes.extend(transcript_lifecycle.deletes);
    groups.deletes.extend(pronunciation_lifecycle.deletes);
    groups.deletes.extend(api_lifecycle.operation_deletes);

    groups.creates.extend(variant_lifecycle.variant_creates);
    groups.creates.extend(variant_lifecycle.attribute_creates);
    groups.creates.extend(api_lifecycle.integration_creates);
    groups.creates.extend(keyphrase_lifecycle.creates);
    groups.creates.extend(transcript_lifecycle.creates);
    groups.creates.extend(pronunciation_lifecycle.creates);
    groups.creates.extend(api_lifecycle.operation_creates);

    groups.updates.extend(api_lifecycle.integration_updates);
    groups.updates.extend(variant_lifecycle.variant_updates);
    groups.updates.extend(variant_lifecycle.attribute_updates);
    groups.updates.extend(keyphrase_lifecycle.updates);
    groups.updates.extend(transcript_lifecycle.updates);
    groups.updates.extend(pronunciation_lifecycle.updates);
    groups.updates.extend(api_lifecycle.operation_updates);
    groups.updates.extend(api_lifecycle.config_updates);

    groups
}

fn entity_aggregate_command_groups(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> CommandGroups {
    let remote_entities = entity_entries(projection)
        .into_iter()
        .map(|(id, entity)| {
            let name = entity
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(id.as_str())
                .to_string();
            (name, (id, entity))
        })
        .collect::<HashMap<_, _>>();
    let mut local_entity_names = HashSet::new();
    let mut groups = CommandGroups::default();
    for resource in resources.values() {
        let path = resource.file_path.as_str();
        let content = resource
            .payload
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();

        if path == ENTITIES_FILE.file_path
            && let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(content)
            && let Some(items) = yaml
                .get("entities")
                .and_then(serde_yaml::Value::as_sequence)
        {
            for item in items {
                let name = item
                    .get("name")
                    .and_then(serde_yaml::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if name.is_empty() {
                    continue;
                }
                local_entity_names.insert(name.clone());
                let remote = remote_entities.get(&name);
                let id = remote
                    .map(|(id, _)| id.clone())
                    .or_else(|| {
                        (!is_synthetic_local_resource_id(&resource.resource_id))
                            .then_some(resource.resource_id.clone())
                    })
                    .unwrap_or_else(|| {
                        stable_resource_id(ENTITY_ID_PREFIX, &name, ENTITIES_FILE.file_path)
                    });
                let entity_type = item
                    .get("entity_type")
                    .and_then(serde_yaml::Value::as_str)
                    .unwrap_or("free_text");
                let description = item
                    .get("description")
                    .and_then(serde_yaml::Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let config = item.get("config");
                if let Some((_, remote_entity)) = remote {
                    if entity_item_matches_remote(item, remote_entity) {
                        continue;
                    }
                    push_command(
                        &mut groups.updates,
                        metadata,
                        "entity_update",
                        CommandPayload::EntityUpdate(EntityUpdate {
                            id: id.clone(),
                            name: name.clone(),
                            r#type: to_camel_case(entity_type),
                            description: description.clone(),
                            references: None,
                            config: build_entity_update_config(entity_type, config),
                        }),
                    );
                } else {
                    push_command(
                        &mut groups.creates,
                        metadata,
                        "entity_create",
                        CommandPayload::EntityCreate(EntityCreate {
                            id: id.clone(),
                            name: name.clone(),
                            r#type: to_camel_case(entity_type),
                            description: description.clone(),
                            references: None,
                            config: build_entity_create_config(entity_type, config),
                        }),
                    );
                }
            }
        }
    }

    for (name, (id, _)) in remote_entities {
        if !local_entity_names.contains(&name) {
            push_command(
                &mut groups.deletes,
                metadata,
                "entity_delete",
                CommandPayload::EntityDelete(EntityDelete { id }),
            );
        }
    }

    groups
}

fn entity_item_matches_remote(item: &serde_yaml::Value, remote: &Value) -> bool {
    let local_name = item
        .get("name")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or_default();
    let remote_name = remote
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if local_name != remote_name {
        return false;
    }

    let local_description = item
        .get("description")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or_default();
    let remote_description = remote
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if local_description != remote_description {
        return false;
    }

    let local_type = normalize_entity_type(
        item.get("entity_type")
            .and_then(serde_yaml::Value::as_str)
            .unwrap_or("free_text"),
    );
    let remote_type =
        normalize_entity_type(remote.get("type").and_then(Value::as_str).unwrap_or(""));
    if local_type != remote_type {
        return false;
    }

    let local_config = build_entity_update_config(&local_type, item.get("config"));
    let remote_config_yaml = remote_entity_config_yaml(remote);
    let remote_config = build_entity_update_config(&remote_type, Some(&remote_config_yaml));
    entity_update_config_json(local_config.as_ref())
        == entity_update_config_json(remote_config.as_ref())
}

fn remote_entity_config_yaml(remote: &Value) -> serde_yaml::Value {
    let config = remote
        .pointer("/config/value")
        .or_else(|| remote.get("config"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    serde_yaml::to_value(config).unwrap_or(serde_yaml::Value::Mapping(Default::default()))
}

fn normalize_entity_type(value: &str) -> String {
    let mut out = String::new();
    for (index, ch) in value.chars().enumerate() {
        if ch == '-' {
            out.push('_');
        } else if ch.is_ascii_uppercase() {
            if index > 0 && !out.ends_with('_') {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

pub(crate) fn payload_json_summary(payload: &CommandPayload) -> Option<(&'static str, Value)> {
    entity_payload_json_summary(payload)
        .or_else(|| interactions::payload_json_summary(payload))
        .or_else(|| summaries::payload_json_summary(payload))
}

fn entity_payload_json_summary(payload: &CommandPayload) -> Option<(&'static str, Value)> {
    match payload {
        CommandPayload::EntityDelete(delete) => Some((
            "entity_delete",
            serde_json::json!({
                "id": delete.id,
            }),
        )),
        CommandPayload::EntityCreate(create) => {
            let mut value = serde_json::Map::new();
            value.insert("id".to_string(), Value::String(create.id.clone()));
            value.insert("name".to_string(), Value::String(create.name.clone()));
            value.insert("type".to_string(), Value::String(create.r#type.clone()));
            value.insert(
                "description".to_string(),
                Value::String(create.description.clone()),
            );
            value.insert(
                "references".to_string(),
                Value::Object(serde_json::Map::new()),
            );
            if let Some((key, config)) = entity_create_config_json(create.config.as_ref()) {
                value.insert(key.to_string(), config);
            }
            Some(("entity_create", Value::Object(value)))
        }
        CommandPayload::EntityUpdate(update) => {
            let mut value = serde_json::Map::new();
            value.insert("id".to_string(), Value::String(update.id.clone()));
            value.insert("name".to_string(), Value::String(update.name.clone()));
            value.insert("type".to_string(), Value::String(update.r#type.clone()));
            value.insert(
                "description".to_string(),
                Value::String(update.description.clone()),
            );
            if let Some((key, config)) = entity_update_config_json(update.config.as_ref()) {
                value.insert(key.to_string(), config);
            }
            Some(("entity_update", Value::Object(value)))
        }
        _ => None,
    }
}

fn entity_create_config_json(
    config: Option<&entities::entity_create::Config>,
) -> Option<(&'static str, Value)> {
    match config? {
        entities::entity_create::Config::Numeric(config) => {
            Some(("numeric", number_config_json(config)))
        }
        entities::entity_create::Config::Alphanumeric(config) => Some((
            "alphanumeric",
            serde_json::json!({
                "enabled": config.enabled,
                "validation_type": config.validation_type,
                "regular_expression": config.regular_expression,
            }),
        )),
        entities::entity_create::Config::Enum(config) => {
            Some(("enum", serde_json::json!({ "options": config.options })))
        }
        entities::entity_create::Config::Date(config) => Some((
            "date",
            serde_json::json!({ "relative_date": config.relative_date }),
        )),
        entities::entity_create::Config::PhoneNumber(config) => Some((
            "phone_number",
            serde_json::json!({
                "enabled": config.enabled,
                "country_codes": config.country_codes,
            }),
        )),
        entities::entity_create::Config::Time(config) => Some((
            "time",
            serde_json::json!({
                "enabled": config.enabled,
                "start_time": config.start_time,
                "end_time": config.end_time,
            }),
        )),
        entities::entity_create::Config::Address(_) => Some(("address", serde_json::json!({}))),
        entities::entity_create::Config::FreeText(_) => Some(("free_text", serde_json::json!({}))),
        entities::entity_create::Config::NameConfig(_) => {
            Some(("name_config", serde_json::json!({})))
        }
    }
}

fn entity_update_config_json(
    config: Option<&entities::entity_update::Config>,
) -> Option<(&'static str, Value)> {
    match config? {
        entities::entity_update::Config::Numeric(config) => {
            Some(("numeric", number_config_json(config)))
        }
        entities::entity_update::Config::Alphanumeric(config) => Some((
            "alphanumeric",
            serde_json::json!({
                "enabled": config.enabled,
                "validation_type": config.validation_type,
                "regular_expression": config.regular_expression,
            }),
        )),
        entities::entity_update::Config::Enum(config) => {
            Some(("enum", serde_json::json!({ "options": config.options })))
        }
        entities::entity_update::Config::Date(config) => Some((
            "date",
            serde_json::json!({ "relative_date": config.relative_date }),
        )),
        entities::entity_update::Config::PhoneNumber(config) => Some((
            "phone_number",
            serde_json::json!({
                "enabled": config.enabled,
                "country_codes": config.country_codes,
            }),
        )),
        entities::entity_update::Config::Time(config) => Some((
            "time",
            serde_json::json!({
                "enabled": config.enabled,
                "start_time": config.start_time,
                "end_time": config.end_time,
            }),
        )),
        entities::entity_update::Config::Address(_) => Some(("address", serde_json::json!({}))),
        entities::entity_update::Config::FreeText(_) => Some(("free_text", serde_json::json!({}))),
        entities::entity_update::Config::NameConfig(_) => {
            Some(("name_config", serde_json::json!({})))
        }
    }
}

fn number_config_json(config: &entities::NumberConfig) -> Value {
    let mut value = serde_json::Map::new();
    if config.has_decimal {
        value.insert("has_decimal".to_string(), Value::Bool(true));
    }
    if config.has_range {
        value.insert("has_range".to_string(), Value::Bool(true));
    }
    if let Some(min) = config.min {
        value.insert("min".to_string(), serde_json::json!(min));
    }
    if let Some(max) = config.max {
        value.insert("max".to_string(), serde_json::json!(max));
    }
    Value::Object(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::local_resource;

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
    fn aggregate_files_emit_real_create_commands() {
        let mut resources = ResourceMap::new();
        resources.insert(
            "config/variant_attributes.yaml".to_string(),
            local_resource(
                "config/variant_attributes.yaml",
                "variant_attributes",
                r#"
variants:
  - name: default
  - name: treatment
attributes:
  - name: adk-recording-cohort
    values:
      default: control
      treatment: treatment
"#,
            ),
        );
        resources.insert(
            "config/api_integrations.yaml".to_string(),
            local_resource(
                "config/api_integrations.yaml",
                "api_integrations",
                r#"
api_integrations:
  - name: adk_recording_api
    description: Recording-only API integration.
    environments:
      sandbox:
        base_url: https://example.invalid/sandbox
        auth_type: none
    operations:
      - name: get_recording_status
        method: GET
        resource: /status
  - name: adk_recording_api_backup
    description: Recording-only backup API integration.
    operations:
      - name: get_recording_status
        method: GET
        resource: /backup/status
"#,
            ),
        );
        resources.insert(
            "voice/speech_recognition/keyphrase_boosting.yaml".to_string(),
            local_resource(
                "voice/speech_recognition/keyphrase_boosting.yaml",
                "keyphrase_boosting",
                "keyphrases:\n  - keyphrase: ADK parity\n    level: boosted\n",
            ),
        );
        resources.insert(
            "voice/speech_recognition/transcript_corrections.yaml".to_string(),
            local_resource(
                "voice/speech_recognition/transcript_corrections.yaml",
                "transcript_corrections",
                r#"
corrections:
  - name: ADK spelling
    description: Correct ADK spelling.
    regular_expressions:
      - regular_expression: agent development kid
        replacement: agent development kit
        replacement_type: full
"#,
            ),
        );
        resources.insert(
            "voice/response_control/pronunciations.yaml".to_string(),
            local_resource(
                "voice/response_control/pronunciations.yaml",
                "pronunciations",
                r#"
pronunciations:
  - regex: \bADK\b
    replacement: Agent Development Kit
    case_sensitive: true
    language_code: en-US
"#,
            ),
        );

        let commands = flatten(aggregate_command_groups(
            &resources,
            &serde_json::json!({}),
            &None,
        ));
        let types = commands
            .iter()
            .map(|command| command.r#type.as_str())
            .collect::<Vec<_>>();
        for expected in [
            "variant_create_variant",
            "variant_create_attribute",
            "create_api_integration",
            "create_api_integration_operation",
            "create_keyphrase_boosting",
            "create_transcript_corrections",
            "pronunciations_create_pronunciation",
        ] {
            assert!(
                types.contains(&expected),
                "missing aggregate-file create command: {expected}"
            );
        }

        let attribute = commands
            .iter()
            .find(|command| command.r#type == "variant_create_attribute")
            .expect("variant_create_attribute command");
        match &attribute.payload {
            Some(CommandPayload::VariantCreateAttribute(payload)) => {
                let values = payload
                    .variant_values
                    .as_ref()
                    .expect("variant values")
                    .values
                    .values()
                    .cloned()
                    .collect::<Vec<_>>();
                assert!(values.contains(&"control".to_string()));
                assert!(values.contains(&"treatment".to_string()));
            }
            _ => panic!("unexpected payload variant for variant_create_attribute command"),
        }

        let api = commands
            .iter()
            .find(|command| command.r#type == "create_api_integration")
            .expect("create_api_integration command");
        match &api.payload {
            Some(CommandPayload::CreateApiIntegration(payload)) => {
                assert_eq!(payload.name, "adk_recording_api");
                assert_eq!(
                    payload
                        .environments
                        .as_ref()
                        .and_then(|envs| envs.sandbox.as_ref())
                        .map(|env| env.base_url.as_str()),
                    Some("https://example.invalid/sandbox")
                );
            }
            _ => panic!("unexpected payload variant for create_api_integration command"),
        }

        let operation_ids = commands
            .iter()
            .filter(|command| command.r#type == "create_api_integration_operation")
            .filter_map(|command| match &command.payload {
                Some(CommandPayload::CreateApiIntegrationOperation(payload)) => {
                    Some(payload.id.as_str())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(operation_ids.len(), 2);
        assert_ne!(operation_ids[0], operation_ids[1]);
    }
}
