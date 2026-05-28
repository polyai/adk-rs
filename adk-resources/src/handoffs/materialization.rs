use crate::materialization::to_yaml_string;
use crate::{CommandGenError, extract_entities_vec};
use adk_types::ResourceMap;
use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
struct HandoffsYaml {
    handoffs: Vec<HandoffYaml>,
}

#[derive(Serialize)]
struct HandoffYaml {
    name: String,
    description: String,
    is_default: bool,
    sip_config: Value,
    sip_headers: Value,
}

pub(crate) fn insert_handoff_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    let mut handoffs = Vec::new();
    for (_id, handoff) in handoff_entries_vec(projection) {
        if !handoff
            .get("active")
            .and_then(Value::as_bool)
            .unwrap_or(true)
        {
            continue;
        }
        handoffs.push(HandoffYaml {
            name: handoff
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            description: handoff
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            is_default: handoff
                .get("isDefault")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            sip_config: handoff_sip_config_yaml(&handoff),
            sip_headers: handoff_sip_headers_yaml(&handoff),
        });
    }
    if handoffs.is_empty() {
        return Ok(());
    }

    let content = to_yaml_string(&HandoffsYaml { handoffs })
        .map_err(|e| CommandGenError::InvalidData(e.to_string()))?;
    crate::materialization::insert_content_resource(
        map,
        "config/handoffs.yaml",
        "handoffs",
        "handoffs",
        content,
    )
}

fn handoff_entries_vec(projection: &Value) -> Vec<(String, Value)> {
    extract_entities_vec(projection, &["handoff", "handoffs", "entities"])
}

fn handoff_sip_config_yaml(handoff: &Value) -> Value {
    let sip_config = handoff.get("sipConfig");
    let config = sip_config
        .and_then(|v| v.get("config"))
        .or(sip_config)
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(case) = config.get("$case").and_then(Value::as_str) {
        let value = config.get("value").unwrap_or(&Value::Null);
        return match case {
            "invite" => serde_json::json!({
                "method": "invite",
                "phone_number": value.get("phoneNumber").and_then(Value::as_str).unwrap_or(""),
                "outbound_endpoint": value.get("outboundEndpoint").and_then(Value::as_str).unwrap_or(""),
                "outbound_encryption": value.get("outboundEncryption").and_then(Value::as_str).unwrap_or(""),
            }),
            "refer" => serde_json::json!({
                "method": "refer",
                "phone_number": value.get("phoneNumber").and_then(Value::as_str).unwrap_or(""),
            }),
            _ => serde_json::json!({ "method": "bye" }),
        };
    }
    if let Some(invite) = config.get("invite") {
        return serde_json::json!({
            "method": "invite",
            "phone_number": invite.get("phoneNumber").and_then(Value::as_str).unwrap_or(""),
            "outbound_endpoint": invite.get("outboundEndpoint").and_then(Value::as_str).unwrap_or(""),
            "outbound_encryption": invite.get("outboundEncryption").and_then(Value::as_str).unwrap_or(""),
        });
    }
    if let Some(refer) = config.get("refer") {
        return serde_json::json!({
            "method": "refer",
            "phone_number": refer.get("phoneNumber").and_then(Value::as_str).unwrap_or(""),
        });
    }
    serde_json::json!({ "method": "bye" })
}

fn handoff_sip_headers_yaml(handoff: &Value) -> Value {
    let headers = handoff
        .get("sipHeaders")
        .and_then(|v| v.get("headers").or(Some(v)))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let yaml_headers = headers
        .iter()
        .map(|h| {
            serde_json::json!({
                "key": h.get("key").and_then(Value::as_str).unwrap_or(""),
                "value": h.get("value").and_then(Value::as_str).unwrap_or(""),
            })
        })
        .collect::<Vec<_>>();
    Value::Array(yaml_headers)
}
