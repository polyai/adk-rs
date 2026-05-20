use super::insert_content_resource;
use crate::yaml_resources::{
    EnvPhoneNumbersYaml, HandoffYaml, HandoffsYaml, PhraseFilterYaml, PhraseFilteringYaml,
    SmsTemplateYaml, SmsTemplatesYaml, to_yaml_string,
};
use crate::{CommandGenError, command_gen, extract_entities_vec};
use adk_types::ResourceMap;
use serde_json::Value;
use std::collections::HashMap;

pub(super) fn insert_synthetic_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    insert_handoffs_resource(map, projection)?;
    insert_sms_templates_resource(map, projection)?;
    insert_phrase_filtering_resource(map, projection)?;
    insert_experimental_config_resource(map, projection)?;
    Ok(())
}

fn insert_handoffs_resource(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    let mut handoff_yaml_list = Vec::new();
    for (_id, handoff) in handoff_entries_vec(projection) {
        if !handoff
            .get("active")
            .and_then(Value::as_bool)
            .unwrap_or(true)
        {
            continue;
        }
        handoff_yaml_list.push(HandoffYaml {
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
    if !handoff_yaml_list.is_empty() {
        let content = to_yaml_string(&HandoffsYaml {
            handoffs: handoff_yaml_list,
        })
        .map_err(|e| CommandGenError::InvalidData(e.to_string()))?;
        insert_content_resource(map, "config/handoffs.yaml", "handoffs", "handoffs", content)?;
    }

    Ok(())
}

fn insert_sms_templates_resource(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    let mut sms_yaml_list = Vec::new();
    for (_id, sms) in sms_entries_vec(projection) {
        if !sms.get("active").and_then(Value::as_bool).unwrap_or(true) {
            continue;
        }
        sms_yaml_list.push(SmsTemplateYaml {
            name: sms
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            text: sms
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            env_phone_numbers: EnvPhoneNumbersYaml {
                sandbox: sms
                    .get("envPhoneNumbers")
                    .and_then(|v| v.get("sandbox"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                pre_release: sms
                    .get("envPhoneNumbers")
                    .and_then(|v| v.get("preRelease"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                live: sms
                    .get("envPhoneNumbers")
                    .and_then(|v| v.get("live"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            },
        });
    }
    if !sms_yaml_list.is_empty() {
        let content = to_yaml_string(&SmsTemplatesYaml {
            sms_templates: sms_yaml_list,
        })
        .map_err(|e| CommandGenError::InvalidData(e.to_string()))?;
        insert_content_resource(
            map,
            "config/sms_templates.yaml",
            "sms_templates",
            "sms_templates",
            content,
        )?;
    }

    Ok(())
}

fn insert_phrase_filtering_resource(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    let global_function_names = command_gen::functions::function_entries(projection)
        .into_iter()
        .filter_map(|(id, function)| {
            Some((
                id,
                function
                    .get("name")?
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            ))
        })
        .collect::<HashMap<_, _>>();
    let mut phrase_yaml_list = Vec::new();
    for (_id, pf) in phrase_filter_entries_vec(projection) {
        let function = pf
            .pointer("/references/globalFunctions")
            .or_else(|| pf.pointer("/references/global_functions"))
            .and_then(Value::as_object)
            .and_then(|refs| refs.keys().next())
            .map(|function_id| {
                global_function_names
                    .get(function_id)
                    .filter(|name| !name.is_empty())
                    .cloned()
                    .unwrap_or_else(|| function_id.to_string())
            });
        phrase_yaml_list.push(PhraseFilterYaml {
            name: pf
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            description: pf
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            regular_expressions: pf
                .get("regularExpressions")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            say_phrase: pf
                .get("sayPhrase")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            language_code: pf
                .get("languageCode")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            function,
        });
    }
    if !phrase_yaml_list.is_empty() {
        let content = to_yaml_string(&PhraseFilteringYaml {
            phrase_filtering: phrase_yaml_list,
        })
        .map_err(|e| CommandGenError::InvalidData(e.to_string()))?;
        insert_content_resource(
            map,
            "voice/response_control/phrase_filtering.yaml",
            "phrase_filtering",
            "phrase_filtering",
            content,
        )?;
    }

    Ok(())
}

fn insert_experimental_config_resource(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    if let Some(features) = experimental_features(projection) {
        let content = serde_json::to_string_pretty(&features)
            .map_err(|e| CommandGenError::InvalidData(e.to_string()))?;
        insert_content_resource(
            map,
            "agent_settings/experimental_config.json",
            "experimental_config",
            "experimental_config",
            content,
        )?;
    }

    Ok(())
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
    let yaml_headers: Vec<Value> = headers
        .iter()
        .map(|h| {
            serde_json::json!({
                "key": h.get("key").and_then(Value::as_str).unwrap_or(""),
                "value": h.get("value").and_then(Value::as_str).unwrap_or(""),
            })
        })
        .collect();
    Value::Array(yaml_headers)
}

fn handoff_entries_vec(projection: &Value) -> Vec<(String, Value)> {
    extract_entities_vec(projection, &["handoff", "handoffs", "entities"])
}

fn sms_entries_vec(projection: &Value) -> Vec<(String, Value)> {
    extract_entities_vec(projection, &["sms", "templates", "entities"])
}

fn phrase_filter_entries_vec(projection: &Value) -> Vec<(String, Value)> {
    extract_entities_vec(projection, &["stopKeywords", "filters", "entities"])
}

fn experimental_features(projection: &Value) -> Option<Value> {
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
