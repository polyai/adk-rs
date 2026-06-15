use crate::handoffs::local::{HANDOFFS_FILE_PATH, Handoff, HandoffsFile, SipConfig};
use crate::materialization::to_yaml_string;
use crate::{CommandGenError, extract_entities_vec};
use adk_types::ResourceMap;
use serde_json::Value;

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
        handoffs.push(local_handoff_from_projection(&handoff)?);
    }
    if handoffs.is_empty() {
        return Ok(());
    }

    let content = to_yaml_string(&HandoffsFile::new(handoffs))
        .map_err(|e| CommandGenError::InvalidData(e.to_string()))?;
    crate::materialization::insert_content_resource(
        map,
        HANDOFFS_FILE_PATH,
        "handoffs",
        "handoffs",
        content,
    )
}

fn handoff_entries_vec(projection: &Value) -> Vec<(String, Value)> {
    extract_entities_vec(projection, &["handoff", "handoffs", "entities"])
}

fn local_handoff_from_projection(handoff: &Value) -> Result<Handoff, CommandGenError> {
    let name = json_str(handoff, "name");
    let description = json_str(handoff, "description");
    let is_default = handoff
        .get("isDefault")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let sip_headers = handoff_sip_headers(handoff);
    let sip_config = handoff_sip_config(handoff);

    Handoff::new(name, description, is_default, sip_config, sip_headers)
        .map_err(invalid_handoff_projection)
}

fn handoff_sip_config(handoff: &Value) -> SipConfig {
    let sip_config = handoff.get("sipConfig");
    let config = sip_config
        .and_then(|v| v.get("config"))
        .or(sip_config)
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(case) = config.get("$case").and_then(Value::as_str) {
        let value = config.get("value").unwrap_or(&Value::Null);
        return match case {
            "invite" => invite_sip_config(value),
            "refer" => refer_sip_config(value),
            _ => SipConfig::Bye,
        };
    }
    if let Some(invite) = config.get("invite") {
        return invite_sip_config(invite);
    }
    if let Some(refer) = config.get("refer") {
        return refer_sip_config(refer);
    }
    SipConfig::Bye
}

fn invite_sip_config(value: &Value) -> SipConfig {
    SipConfig::Invite {
        phone_number: json_str(value, "phoneNumber"),
        outbound_endpoint: json_str(value, "outboundEndpoint"),
        outbound_encryption: json_str(value, "outboundEncryption"),
    }
}

fn refer_sip_config(value: &Value) -> SipConfig {
    SipConfig::Refer {
        phone_number: json_str(value, "phoneNumber"),
    }
}

fn handoff_sip_headers(handoff: &Value) -> Vec<(String, String)> {
    let headers = handoff
        .get("sipHeaders")
        .and_then(|v| v.get("headers").or(Some(v)))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    headers
        .iter()
        .map(|h| {
            (
                h.get("key")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                h.get("value")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            )
        })
        .collect()
}

fn json_str(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn invalid_handoff_projection(error: String) -> CommandGenError {
    CommandGenError::InvalidData(format!("Invalid handoff projection: {error}"))
}
