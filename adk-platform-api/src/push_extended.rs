//! Push commands for variables, handoffs, SMS templates, phrase filters (stop keywords), and
//! experimental config. Mirrors Python `poly.resources.*` command type strings.
//!
//! **Execution ordering:** Python `SyncClientHandler.queue_resources` walks deletes (respecting
//! `PRIORITY_DELETE_TYPES`), then creates (`PRIORITY_CREATE_TYPES`), then updates
//! (`PRIORITY_UPDATE_TYPES`), and finally appends `handoff_set_default` for default handoffs.
//! This module emits per-family command groups; `build_phase1_commands` applies the global
//! delete/create/update ordering across both phase-1 and extended families.

use crate::{actor_identity, clean_name, extract_entities_map, push_command};
use adk_domain::ResourceMap;
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::experimental_config::ExperimentalConfigUpdateConfig;
use adk_protobuf::handoff::{
    HandoffCreate, HandoffDelete, HandoffSetDefault, HandoffUpdate, SipByeHandoffConfig, SipConfig,
    SipHeader, SipHeaders, SipInviteHandoffConfig, SipReferHandoffConfig, sip_config,
};
use adk_protobuf::sms::{
    SmsCreateTemplate, SmsDeleteTemplate, SmsEnvPhoneNumbers, SmsTemplateReferences,
    SmsUpdateTemplate, UpdateSmsEnvPhoneNumbers,
};
use adk_protobuf::stop_keywords::{
    StopKeywordCreate, StopKeywordDelete, StopKeywordReferences, StopKeywordUpdate,
};
use adk_protobuf::variables::{VariableCreate, VariableDelete, VariableUpdate};
use adk_protobuf::{Command, Metadata};
use prost_types::value::Kind;
use prost_types::{ListValue, Struct, Value as ProstValue};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Default)]
pub(crate) struct CommandGroups {
    pub deletes: Vec<Command>,
    pub creates: Vec<Command>,
    pub updates: Vec<Command>,
    pub post_updates: Vec<Command>,
}

/// JSON object ? `google.protobuf.Struct` (for experimental config `features`).
fn json_to_prost_struct(v: &Value) -> Option<Struct> {
    let obj = v.as_object()?;
    let mut fields = BTreeMap::new();
    for (k, val) in obj {
        fields.insert(k.clone(), json_to_prost_value(val));
    }
    Some(Struct { fields })
}

fn json_to_prost_value(v: &Value) -> ProstValue {
    match v {
        Value::Null => ProstValue {
            kind: Some(Kind::NullValue(0)),
        },
        Value::Bool(b) => ProstValue {
            kind: Some(Kind::BoolValue(*b)),
        },
        Value::Number(n) => ProstValue {
            kind: Some(Kind::NumberValue(n.as_f64().unwrap_or(0.0))),
        },
        Value::String(s) => ProstValue {
            kind: Some(Kind::StringValue(s.clone())),
        },
        Value::Array(arr) => ProstValue {
            kind: Some(Kind::ListValue(ListValue {
                values: arr.iter().map(json_to_prost_value).collect(),
            })),
        },
        Value::Object(obj) => {
            let mut fields = BTreeMap::new();
            for (k, v) in obj {
                fields.insert(k.clone(), json_to_prost_value(v));
            }
            ProstValue {
                kind: Some(Kind::StructValue(Struct { fields })),
            }
        }
    }
}

fn sdk_user(metadata: &Option<Metadata>) -> String {
    metadata
        .as_ref()
        .map(|m| m.created_by.trim())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(actor_identity)
}

fn remote_variables(projection: &Value) -> HashMap<String, String> {
    let entities = extract_entities_map(projection, &["variables", "variables", "entities"]);
    let mut m = HashMap::new();
    for (id, v) in entities {
        let name = v
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&id)
            .to_string();
        m.insert(name, id);
    }
    m
}

fn remote_handoffs(projection: &Value) -> HashMap<String, String> {
    let entities = extract_entities_map(projection, &["handoff", "handoffs", "entities"]);
    let mut m = HashMap::new();
    for (id, v) in entities {
        if !v.get("active").and_then(Value::as_bool).unwrap_or(true) {
            continue;
        }
        let name = v
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&id)
            .to_string();
        m.insert(name, id);
    }
    m
}

fn remote_sms(projection: &Value) -> HashMap<String, String> {
    let entities = extract_entities_map(projection, &["sms", "templates", "entities"]);
    let mut m = HashMap::new();
    for (id, v) in entities {
        if !v.get("active").and_then(Value::as_bool).unwrap_or(false) {
            continue;
        }
        let name = v
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&id)
            .to_string();
        m.insert(name, id);
    }
    m
}

fn remote_phrase_filters(projection: &Value) -> HashMap<String, String> {
    let entities = extract_entities_map(projection, &["stopKeywords", "filters", "entities"]);
    let mut m = HashMap::new();
    for (id, v) in entities {
        let title = v
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or(&id)
            .to_string();
        m.insert(title, id);
    }
    m
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

fn yaml_str(y: &serde_yaml::Value, key: &str) -> String {
    y.get(key)
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn handoff_sip_config(y: &serde_yaml::Value) -> SipConfig {
    let sc = match y.get("sip_config").or_else(|| y.get("sipConfig")) {
        Some(v) => v,
        None => {
            return SipConfig {
                config: Some(sip_config::Config::Bye(SipByeHandoffConfig {})),
            };
        }
    };
    let method = sc
        .get("method")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or("bye");
    let cfg = match method {
        "invite" => sip_config::Config::Invite(SipInviteHandoffConfig {
            phone_number: yaml_str(sc, "phone_number"),
            outbound_endpoint: yaml_str(sc, "outbound_endpoint"),
            outbound_encryption: yaml_str(sc, "outbound_encryption"),
        }),
        "refer" => sip_config::Config::Refer(SipReferHandoffConfig {
            phone_number: yaml_str(sc, "phone_number"),
        }),
        _ => sip_config::Config::Bye(SipByeHandoffConfig {}),
    };
    SipConfig { config: Some(cfg) }
}

fn sip_headers_from_yaml(y: &serde_yaml::Value) -> Option<SipHeaders> {
    let arr = y
        .get("sip_headers")
        .or_else(|| y.get("sipHeaders"))?
        .as_sequence()?;
    let mut headers = Vec::new();
    for row in arr {
        let key = row.get("key")?.as_str()?;
        let value = row.get("value")?.as_str()?;
        headers.push(SipHeader {
            key: key.to_string(),
            value: value.to_string(),
        });
    }
    if headers.is_empty() {
        None
    } else {
        Some(SipHeaders { headers })
    }
}

fn sms_env_phone_numbers(y: &serde_yaml::Value) -> SmsEnvPhoneNumbers {
    let ep = y
        .get("env_phone_numbers")
        .or_else(|| y.get("envPhoneNumbers"));
    let ep = match ep {
        Some(v) => v,
        None => {
            return SmsEnvPhoneNumbers {
                sandbox: String::new(),
                pre_release: String::new(),
                live: String::new(),
            };
        }
    };
    let pre = yaml_str(ep, "pre_release");
    let pre = if pre.is_empty() {
        yaml_str(ep, "preRelease")
    } else {
        pre
    };
    SmsEnvPhoneNumbers {
        sandbox: yaml_str(ep, "sandbox"),
        pre_release: pre,
        live: yaml_str(ep, "live"),
    }
}

fn sms_env_update(y: &serde_yaml::Value) -> UpdateSmsEnvPhoneNumbers {
    let ep = y
        .get("env_phone_numbers")
        .or_else(|| y.get("envPhoneNumbers"));
    let ep = match ep {
        Some(v) => v,
        None => {
            return UpdateSmsEnvPhoneNumbers {
                sandbox: None,
                pre_release: None,
                live: None,
            };
        }
    };
    let pre = yaml_str(ep, "pre_release");
    let pre = if pre.is_empty() {
        yaml_str(ep, "preRelease")
    } else {
        pre
    };
    UpdateSmsEnvPhoneNumbers {
        sandbox: Some(yaml_str(ep, "sandbox")),
        pre_release: Some(pre),
        live: Some(yaml_str(ep, "live")),
    }
}

fn sms_matches_remote(local: &serde_yaml::Value, remote: &Value) -> bool {
    let ln = local.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let rn = remote.get("name").and_then(Value::as_str).unwrap_or("");
    if ln != rn {
        return false;
    }
    let lt = local.get("text").and_then(|v| v.as_str()).unwrap_or("");
    let rt = remote.get("text").and_then(Value::as_str).unwrap_or("");
    if lt != rt {
        return false;
    }
    let l_ep = local
        .get("env_phone_numbers")
        .or_else(|| local.get("envPhoneNumbers"));
    let r_ep = remote.get("envPhoneNumbers");
    let ls = l_ep
        .and_then(|x| x.get("sandbox"))
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or("");
    let rs = r_ep
        .and_then(|x| x.get("sandbox"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let mut lp = l_ep
        .and_then(|x| x.get("pre_release"))
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or("");
    if lp.is_empty() {
        lp = l_ep
            .and_then(|x| x.get("preRelease"))
            .and_then(serde_yaml::Value::as_str)
            .unwrap_or("");
    }
    let mut rp = r_ep
        .and_then(|x| x.get("preRelease"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if rp.is_empty() {
        rp = r_ep
            .and_then(|x| x.get("pre_release"))
            .and_then(Value::as_str)
            .unwrap_or("");
    }
    let ll = l_ep
        .and_then(|x| x.get("live"))
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or("");
    let rl = r_ep
        .and_then(|x| x.get("live"))
        .and_then(Value::as_str)
        .unwrap_or("");
    ls == rs && lp == rp && ll == rl
}

fn yaml_reference_map(y: Option<&serde_yaml::Value>) -> HashMap<String, bool> {
    let Some(y) = y else {
        return HashMap::new();
    };
    if let Some(arr) = y.as_sequence() {
        return arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| (s.to_string(), true)))
            .collect();
    }
    if let Some(obj) = y.as_mapping() {
        let mut out = HashMap::new();
        for (k, v) in obj {
            if let Some(key) = k.as_str() {
                out.insert(key.to_string(), v.as_bool().unwrap_or(true));
            }
        }
        return out;
    }
    HashMap::new()
}

fn json_reference_map(y: Option<&Value>) -> HashMap<String, bool> {
    let Some(y) = y else {
        return HashMap::new();
    };
    if let Some(obj) = y.as_object() {
        let mut out = HashMap::new();
        for (k, v) in obj {
            out.insert(k.clone(), v.as_bool().unwrap_or(true));
        }
        return out;
    }
    HashMap::new()
}

fn sms_references_from_yaml(yaml: &serde_yaml::Value) -> Option<SmsTemplateReferences> {
    let refs = yaml.get("references").or_else(|| yaml.get("refs"));
    let topics = yaml_reference_map(refs.and_then(|r| r.get("topics")));
    let flow_steps = yaml_reference_map(refs.and_then(|r| r.get("flow_steps")));
    let variables = yaml_reference_map(refs.and_then(|r| r.get("variables")));
    let translations = yaml_reference_map(refs.and_then(|r| r.get("translations")));
    if topics.is_empty() && flow_steps.is_empty() && variables.is_empty() && translations.is_empty()
    {
        return None;
    }
    Some(SmsTemplateReferences {
        topics,
        flow_steps,
        variables,
        translations,
    })
}

fn sms_references_from_remote(remote: Option<&Value>) -> Option<SmsTemplateReferences> {
    let refs = remote.and_then(|v| v.get("references"))?;
    let topics = json_reference_map(refs.get("topics"));
    let flow_steps = json_reference_map(refs.get("flowSteps").or_else(|| refs.get("flow_steps")));
    let variables = json_reference_map(refs.get("variables"));
    let translations = json_reference_map(refs.get("translations"));
    if topics.is_empty() && flow_steps.is_empty() && variables.is_empty() && translations.is_empty()
    {
        return None;
    }
    Some(SmsTemplateReferences {
        topics,
        flow_steps,
        variables,
        translations,
    })
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

fn phrase_refs_from_yaml(yaml: &serde_yaml::Value) -> Option<StopKeywordReferences> {
    let mut global_functions = HashMap::new();
    if let Some(fid) = yaml.get("function").and_then(serde_yaml::Value::as_str)
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

pub(crate) fn extended_resource_command_groups(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> CommandGroups {
    let mut var_del = Vec::new();
    let mut var_create = Vec::new();
    let mut var_update = Vec::new();
    let mut ho_del = Vec::new();
    let mut ho_create = Vec::new();
    let mut ho_update = Vec::new();
    let mut sms_del = Vec::new();
    let mut sms_create = Vec::new();
    let mut sms_update = Vec::new();
    let mut sk_del = Vec::new();
    let mut sk_create = Vec::new();
    let mut sk_update = Vec::new();
    let mut exp_update = Vec::new();

    let rv = remote_variables(projection);
    let remote_ho = remote_handoffs(projection);
    let rsms = remote_sms(projection);
    let rpf = remote_phrase_filters(projection);

    let mut local_var_names = HashSet::new();
    let mut local_ho_names = HashSet::new();
    let mut local_sms_names = HashSet::new();
    let mut local_pf_titles = HashSet::new();

    for resource in resources.values() {
        let path = resource.file_path.as_str();
        let content = resource
            .payload
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();

        if path.starts_with("variables/") {
            let name = path.trim_start_matches("variables/").to_string();
            if name.is_empty() {
                continue;
            }
            local_var_names.insert(name.clone());
            let id = rv
                .get(&name)
                .cloned()
                .or_else(|| {
                    (!resource.resource_id.trim().is_empty() && resource.resource_id != "local")
                        .then_some(resource.resource_id.clone())
                })
                .unwrap_or_else(|| format!("vrbl-{}", clean_name(&name).to_lowercase()));
            if rv.contains_key(&name) {
                push_command(
                    &mut var_update,
                    metadata,
                    "variable_update",
                    CommandPayload::VariableUpdate(VariableUpdate {
                        id: id.clone(),
                        name: name.clone(),
                        references: None,
                    }),
                );
            } else {
                push_command(
                    &mut var_create,
                    metadata,
                    "variable_create",
                    CommandPayload::VariableCreate(VariableCreate {
                        id: id.clone(),
                        name: name.clone(),
                        references: None,
                    }),
                );
            }
            continue;
        }

        if path == "config/handoffs.yaml" {
            if let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(content)
                && let Some(items) = yaml
                    .get("handoffs")
                    .and_then(serde_yaml::Value::as_sequence)
            {
                for item in items {
                    let name = yaml_str(item, "name");
                    if !name.is_empty() {
                        local_ho_names.insert(name);
                    }
                }
            }
            continue;
        }

        if path.starts_with("config/handoffs.yaml/handoffs/") {
            if let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(content) {
                let name = yaml_str(&yaml, "name");
                if name.is_empty() {
                    continue;
                }
                local_ho_names.insert(name.clone());
                let id = remote_ho
                    .get(&name)
                    .cloned()
                    .or_else(|| {
                        (!resource.resource_id.trim().is_empty() && resource.resource_id != "local")
                            .then_some(resource.resource_id.clone())
                    })
                    .unwrap_or_else(|| format!("ho-{}", clean_name(&name).to_lowercase()));
                let description = yaml_str(&yaml, "description");
                let sip_config = handoff_sip_config(&yaml);
                let sip_headers = sip_headers_from_yaml(&yaml);
                if remote_ho.contains_key(&name) {
                    push_command(
                        &mut ho_update,
                        metadata,
                        "handoff_update",
                        CommandPayload::HandoffUpdate(HandoffUpdate {
                            id: id.clone(),
                            name: Some(name.clone()),
                            description: Some(description),
                            sip_config: Some(sip_config),
                            sip_headers,
                            active: Some(true),
                            references: None,
                        }),
                    );
                } else {
                    push_command(
                        &mut ho_create,
                        metadata,
                        "handoff_create",
                        CommandPayload::HandoffCreate(HandoffCreate {
                            id: id.clone(),
                            name: name.clone(),
                            description,
                            sip_config: Some(sip_config),
                            sip_headers,
                            active: true,
                            references: None,
                        }),
                    );
                }
            }
            continue;
        }

        if path == "config/sms_templates.yaml" {
            if let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(content)
                && let Some(items) = yaml
                    .get("sms_templates")
                    .and_then(serde_yaml::Value::as_sequence)
            {
                for item in items {
                    let name = yaml_str(item, "name");
                    if !name.is_empty() {
                        local_sms_names.insert(name);
                    }
                }
            }
            continue;
        }

        if path.starts_with("config/sms_templates.yaml/sms_templates/") {
            if let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(content) {
                let name = yaml_str(&yaml, "name");
                if name.is_empty() {
                    continue;
                }
                local_sms_names.insert(name.clone());
                let id = rsms
                    .get(&name)
                    .cloned()
                    .or_else(|| {
                        (!resource.resource_id.trim().is_empty() && resource.resource_id != "local")
                            .then_some(resource.resource_id.clone())
                    })
                    .unwrap_or_else(|| format!("twilio_sms-{}", clean_name(&name).to_lowercase()));
                let text = yaml_str(&yaml, "text");
                let env_create = sms_env_phone_numbers(&yaml);
                let env_up = sms_env_update(&yaml);
                let local_refs = sms_references_from_yaml(&yaml);
                if rsms.contains_key(&name) {
                    let sms_entities =
                        extract_entities_map(projection, &["sms", "templates", "entities"]);
                    let mut remote_template: Option<&Value> = None;
                    if let Some(rid) = rsms.get(&name) {
                        if let Some(rem) = sms_entities.get(rid.as_str()) {
                            remote_template = Some(rem);
                            if sms_matches_remote(&yaml, rem) {
                                continue;
                            }
                        }
                    }
                    push_command(
                        &mut sms_update,
                        metadata,
                        "sms_update_template",
                        CommandPayload::SmsUpdateTemplate(SmsUpdateTemplate {
                            id: id.clone(),
                            name: Some(name.clone()),
                            text: Some(text),
                            env_phone_numbers: Some(env_up),
                            references: local_refs
                                .clone()
                                .or_else(|| sms_references_from_remote(remote_template)),
                            active: Some(true),
                        }),
                    );
                } else {
                    push_command(
                        &mut sms_create,
                        metadata,
                        "sms_create_template",
                        CommandPayload::SmsCreateTemplate(SmsCreateTemplate {
                            id: id.clone(),
                            name: name.clone(),
                            text,
                            env_phone_numbers: Some(env_create),
                            references: local_refs,
                            active: true,
                        }),
                    );
                }
            }
            continue;
        }

        if path.starts_with("voice/response_control/phrase_filtering.yaml/phrase_filtering/") {
            if let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(content) {
                let title = yaml_str(&yaml, "name");
                if title.is_empty() {
                    continue;
                }
                local_pf_titles.insert(title.clone());
                let id = rpf
                    .get(&title)
                    .cloned()
                    .or_else(|| {
                        (!resource.resource_id.trim().is_empty() && resource.resource_id != "local")
                            .then_some(resource.resource_id.clone())
                    })
                    .unwrap_or_else(|| format!("sk-{}", clean_name(&title).to_lowercase()));
                let description = yaml_str(&yaml, "description");
                let say_phrase = yaml
                    .get("say_phrase")
                    .or_else(|| yaml.get("sayPhrase"))
                    .and_then(serde_yaml::Value::as_bool)
                    .unwrap_or(false);
                let language_code = yaml_str(&yaml, "language_code");
                let language_code = if language_code.is_empty() {
                    yaml_str(&yaml, "languageCode")
                } else {
                    language_code
                };
                let regular_expressions: Vec<String> = yaml
                    .get("regular_expressions")
                    .or_else(|| yaml.get("regularExpressions"))
                    .and_then(serde_yaml::Value::as_sequence)
                    .map(|seq| {
                        seq.iter()
                            .filter_map(serde_yaml::Value::as_str)
                            .map(ToString::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                let references = phrase_refs_from_yaml(&yaml).or_else(|| {
                    let function_id = yaml
                        .get("function")
                        .and_then(serde_yaml::Value::as_str)
                        .map(ToString::to_string);
                    phrase_refs(function_id.as_deref())
                });

                if rpf.contains_key(&title) {
                    push_command(
                        &mut sk_update,
                        metadata,
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
                        &mut sk_create,
                        metadata,
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
            continue;
        }

        if path == "agent_settings/experimental_config.json" {
            let trimmed = content.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(local_json) = serde_json::from_str::<Value>(trimmed) else {
                continue;
            };
            let remote_f = remote_experimental_features(projection);
            let needs = match &remote_f {
                None => true,
                Some(r) => r != &local_json,
            };
            if needs {
                let id =
                    if !resource.resource_id.trim().is_empty() && resource.resource_id != "local" {
                        resource.resource_id.clone()
                    } else {
                        remote_experimental_config_id(projection)
                            .unwrap_or_else(|| "default".to_string())
                    };
                let features = json_to_prost_struct(&local_json);
                push_command(
                    &mut exp_update,
                    metadata,
                    "experimental_config_update_config",
                    CommandPayload::ExperimentalConfigUpdateConfig(
                        ExperimentalConfigUpdateConfig {
                            id,
                            features,
                            updated_at: None,
                            updated_by: sdk_user(metadata),
                        },
                    ),
                );
            }
        }
    }

    for (name, id) in &rv {
        if !local_var_names.contains(name) {
            push_command(
                &mut var_del,
                metadata,
                "variable_delete",
                CommandPayload::VariableDelete(VariableDelete { id: id.clone() }),
            );
        }
    }
    for (name, id) in &remote_ho {
        if !local_ho_names.contains(name) {
            push_command(
                &mut ho_del,
                metadata,
                "handoff_delete",
                CommandPayload::HandoffDelete(HandoffDelete { id: id.clone() }),
            );
        }
    }
    for (name, id) in &rsms {
        if !local_sms_names.contains(name) {
            push_command(
                &mut sms_del,
                metadata,
                "sms_delete_template",
                CommandPayload::SmsDeleteTemplate(SmsDeleteTemplate { id: id.clone() }),
            );
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

    // `handoff_set_default` is queued after create/update in Python (not part of the three phases).
    let mut defaults: Vec<Command> = Vec::new();
    for resource in resources.values() {
        let path = resource.file_path.as_str();
        if !path.starts_with("config/handoffs.yaml/handoffs/") {
            continue;
        }
        let content = resource
            .payload
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(content) {
            let is_def = yaml
                .get("is_default")
                .or_else(|| yaml.get("isDefault"))
                .and_then(serde_yaml::Value::as_bool)
                .unwrap_or(false);
            if !is_def {
                continue;
            }
            let name = yaml_str(&yaml, "name");
            if name.is_empty() {
                continue;
            }
            let id = remote_ho
                .get(&name)
                .cloned()
                .or_else(|| {
                    (!resource.resource_id.trim().is_empty() && resource.resource_id != "local")
                        .then_some(resource.resource_id.clone())
                })
                .unwrap_or_else(|| format!("ho-{}", clean_name(&name).to_lowercase()));
            push_command(
                &mut defaults,
                metadata,
                "handoff_set_default",
                CommandPayload::HandoffSetDefault(HandoffSetDefault { id }),
            );
        }
    }
    let mut groups = CommandGroups::default();
    groups.deletes.extend(var_del);
    groups.deletes.extend(ho_del);
    groups.deletes.extend(sms_del);
    groups.deletes.extend(sk_del);
    groups.creates.extend(var_create);
    groups.creates.extend(ho_create);
    groups.creates.extend(sms_create);
    groups.creates.extend(sk_create);
    groups.updates.extend(var_update);
    groups.updates.extend(ho_update);
    groups.updates.extend(sms_update);
    groups.updates.extend(sk_update);
    groups.updates.extend(exp_update);
    groups.post_updates = defaults;
    groups
}

#[cfg(test)]
#[path = "push_extended_tests.rs"]
mod tests;
