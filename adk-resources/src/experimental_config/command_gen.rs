use crate::push_commands::default_metadata_created_by;
use crate::specs::EXPERIMENTAL_CONFIG_FILE;
use crate::{is_synthetic_local_resource_id, push_command};
use adk_protobuf::Metadata;
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::experimental_config::ExperimentalConfigUpdateConfig;
use adk_types::ResourceMap;
use prost_types::value::Kind;
use prost_types::{ListValue, Struct, Value as ProstValue};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(crate) fn append_experimental_config_update(
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
    if crate::experimental_config::experimental_features(projection).as_ref() == Some(&local_json) {
        return;
    }
    let id = remote_experimental_config_id(projection).unwrap_or_else(|| {
        if !is_synthetic_local_resource_id(&resource.resource_id) {
            resource.resource_id.clone()
        } else {
            "default".to_string()
        }
    });
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

fn remote_experimental_config_id(projection: &Value) -> Option<String> {
    crate::experimental_config::experimental_config_entry(projection).map(|(id, _)| id)
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
    match payload {
        CommandPayload::ExperimentalConfigUpdateConfig(update) => Some((
            "experimental_config_update_config",
            json!({
                "id": update.id,
                "features": update
                    .features
                    .as_ref()
                    .map(prost_struct_json)
                    .unwrap_or_else(|| json!({})),
            }),
        )),
        _ => None,
    }
}

fn prost_struct_json(value: &Struct) -> Value {
    Value::Object(
        value
            .fields
            .iter()
            .map(|(key, value)| (key.clone(), prost_value_json(value)))
            .collect(),
    )
}

fn prost_value_json(value: &ProstValue) -> Value {
    match value.kind.as_ref() {
        Some(Kind::NullValue(_)) | None => Value::Null,
        Some(Kind::NumberValue(value)) => json!(value),
        Some(Kind::StringValue(value)) => Value::String(value.clone()),
        Some(Kind::BoolValue(value)) => Value::Bool(*value),
        Some(Kind::StructValue(value)) => prost_struct_json(value),
        Some(Kind::ListValue(value)) => {
            Value::Array(value.values.iter().map(prost_value_json).collect())
        }
    }
}

#[cfg(test)]
#[path = "command_gen_tests.rs"]
mod command_gen_tests;
