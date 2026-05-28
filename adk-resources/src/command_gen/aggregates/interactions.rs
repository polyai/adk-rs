//! Push commands for interaction aggregate files.
//!
//! This covers handoffs, SMS templates, and phrase filters (stop keywords), and
//! mirrors Python `poly.resources.*` command type strings.
//!
//! **Execution ordering:** Python `SyncClientHandler.queue_resources` walks deletes (respecting
//! `PRIORITY_DELETE_TYPES`), then creates (`PRIORITY_CREATE_TYPES`), then updates
//! (`PRIORITY_UPDATE_TYPES`), and finally appends `handoff_set_default` for default handoffs.
//! This module emits per-family command groups; `build_push_commands` applies the global
//! delete/create/update ordering across all resource-family modules.

use crate::ids::stable_resource_id;
use crate::{extract_entities_map, is_synthetic_local_resource_id, push_command};
use adk_protobuf::command::Payload as CommandPayload;
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
use adk_protobuf::{Command, Metadata};
use adk_types::ResourceMap;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

use super::super::CommandGroups;

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

struct HandoffItemQueue<'a> {
    projection: &'a Value,
    remote: &'a HashMap<String, String>,
    metadata: &'a Option<Metadata>,
    local_names: &'a mut HashSet<String>,
    creates: &'a mut Vec<Command>,
    updates: &'a mut Vec<Command>,
}

impl HandoffItemQueue<'_> {
    fn queue(&mut self, yaml: &serde_yaml::Value, resource_id: &str) -> Option<String> {
        let name = yaml_str(yaml, "name");
        if name.is_empty() {
            return None;
        }
        self.local_names.insert(name.clone());
        let id = self
            .remote
            .get(&name)
            .cloned()
            .or_else(|| {
                (!is_synthetic_local_resource_id(resource_id)).then_some(resource_id.to_string())
            })
            .unwrap_or_else(|| stable_resource_id("HANDOFFS", &name, "config/handoffs.yaml"));
        let description = yaml_str(yaml, "description");
        let sip_config = handoff_sip_config(yaml);
        let sip_headers = sip_headers_from_yaml(yaml);
        if self.remote.contains_key(&name) {
            if let Some(remote) = self.remote.get(&name).and_then(|id| {
                extract_entities_map(self.projection, &["handoff", "handoffs", "entities"])
                    .get(id)
                    .cloned()
            }) && handoff_matches_remote(yaml, &remote)
            {
                return None;
            }
            push_command(
                self.updates,
                self.metadata,
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
            Some(name)
        } else {
            push_command(
                self.creates,
                self.metadata,
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
            Some(name)
        }
    }
}

struct SmsItemQueue<'a> {
    projection: &'a Value,
    remote: &'a HashMap<String, String>,
    metadata: &'a Option<Metadata>,
    local_names: &'a mut HashSet<String>,
    creates: &'a mut Vec<Command>,
    updates: &'a mut Vec<Command>,
}

impl SmsItemQueue<'_> {
    fn queue(&mut self, yaml: &serde_yaml::Value, resource_id: &str) {
        let name = yaml_str(yaml, "name");
        if name.is_empty() {
            return;
        }
        self.local_names.insert(name.clone());
        let id = self
            .remote
            .get(&name)
            .cloned()
            .or_else(|| {
                (!is_synthetic_local_resource_id(resource_id)).then_some(resource_id.to_string())
            })
            .unwrap_or_else(|| {
                stable_resource_id("SMS_TEMPLATES", &name, "config/sms_templates.yaml")
            });
        let text = yaml_str(yaml, "text");
        let env_create = sms_env_phone_numbers(yaml);
        let env_up = sms_env_update(yaml);
        let local_refs = sms_references_from_yaml(yaml);
        if self.remote.contains_key(&name) {
            let sms_entities =
                extract_entities_map(self.projection, &["sms", "templates", "entities"]);
            let mut remote_template: Option<&Value> = None;
            if let Some(rid) = self.remote.get(&name)
                && let Some(rem) = sms_entities.get(rid.as_str())
            {
                remote_template = Some(rem);
                if sms_matches_remote(yaml, rem) {
                    return;
                }
            }
            push_command(
                self.updates,
                self.metadata,
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
                self.creates,
                self.metadata,
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
}

fn queue_handoff_default_item(
    yaml: &serde_yaml::Value,
    resource_id: &str,
    projection: &Value,
    remote_ho: &HashMap<String, String>,
    changed_ho_names: &HashSet<String>,
    metadata: &Option<Metadata>,
    defaults: &mut Vec<Command>,
) {
    let is_default = yaml
        .get("is_default")
        .or_else(|| yaml.get("isDefault"))
        .and_then(serde_yaml::Value::as_bool)
        .unwrap_or(false);
    if !is_default {
        return;
    }
    let name = yaml_str(yaml, "name");
    if name.is_empty() {
        return;
    }
    if let Some(remote_id) = remote_ho.get(&name)
        && let Some(remote) = extract_entities_map(projection, &["handoff", "handoffs", "entities"])
            .get(remote_id)
            .cloned()
        && remote
            .get("isDefault")
            .or_else(|| remote.get("is_default"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && !changed_ho_names.contains(&name)
    {
        return;
    }
    let id = remote_ho
        .get(&name)
        .cloned()
        .or_else(|| {
            (!is_synthetic_local_resource_id(resource_id)).then_some(resource_id.to_string())
        })
        .unwrap_or_else(|| stable_resource_id("HANDOFFS", &name, "config/handoffs.yaml"));
    push_command(
        defaults,
        metadata,
        "handoff_set_default",
        CommandPayload::HandoffSetDefault(HandoffSetDefault { id }),
    );
}

fn handoff_matches_remote(local: &serde_yaml::Value, remote: &Value) -> bool {
    yaml_str(local, "name") == remote.get("name").and_then(Value::as_str).unwrap_or("")
        && yaml_str(local, "description")
            == remote
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
        && handoff_sip_config(local) == handoff_sip_config_from_remote(remote)
        && sip_headers_from_yaml(local) == sip_headers_from_remote(remote)
}

fn handoff_sip_config_from_remote(remote: &Value) -> SipConfig {
    let Some(config) = remote
        .get("sipConfig")
        .and_then(|value| value.get("config").or(Some(value)))
    else {
        return SipConfig {
            config: Some(sip_config::Config::Bye(SipByeHandoffConfig {})),
        };
    };
    if let Some(case) = config.get("$case").and_then(Value::as_str) {
        let value = config.get("value").unwrap_or(&Value::Null);
        let config = match case {
            "invite" => sip_config::Config::Invite(SipInviteHandoffConfig {
                phone_number: json_str(value, "phoneNumber"),
                outbound_endpoint: json_str(value, "outboundEndpoint"),
                outbound_encryption: json_str(value, "outboundEncryption"),
            }),
            "refer" => sip_config::Config::Refer(SipReferHandoffConfig {
                phone_number: json_str(value, "phoneNumber"),
            }),
            _ => sip_config::Config::Bye(SipByeHandoffConfig {}),
        };
        return SipConfig {
            config: Some(config),
        };
    }
    if let Some(invite) = config.get("invite") {
        return SipConfig {
            config: Some(sip_config::Config::Invite(SipInviteHandoffConfig {
                phone_number: json_str(invite, "phoneNumber"),
                outbound_endpoint: json_str(invite, "outboundEndpoint"),
                outbound_encryption: json_str(invite, "outboundEncryption"),
            })),
        };
    }
    if let Some(refer) = config.get("refer") {
        return SipConfig {
            config: Some(sip_config::Config::Refer(SipReferHandoffConfig {
                phone_number: json_str(refer, "phoneNumber"),
            })),
        };
    }
    SipConfig {
        config: Some(sip_config::Config::Bye(SipByeHandoffConfig {})),
    }
}

fn sip_headers_from_remote(remote: &Value) -> Option<SipHeaders> {
    let headers = remote
        .get("sipHeaders")
        .and_then(|value| value.get("headers").or(Some(value)))
        .and_then(Value::as_array)?;
    let headers = headers
        .iter()
        .filter_map(|header| {
            Some(SipHeader {
                key: header.get("key")?.as_str()?.to_string(),
                value: header.get("value")?.as_str()?.to_string(),
            })
        })
        .collect::<Vec<_>>();
    if headers.is_empty() {
        None
    } else {
        Some(SipHeaders { headers })
    }
}

fn json_str(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

struct PhraseFilterItemQueue<'a> {
    projection: &'a Value,
    remote: &'a HashMap<String, String>,
    metadata: &'a Option<Metadata>,
    local_titles: &'a mut HashSet<String>,
    creates: &'a mut Vec<Command>,
    updates: &'a mut Vec<Command>,
}

impl PhraseFilterItemQueue<'_> {
    fn queue(&mut self, yaml: &serde_yaml::Value, resource_id: &str) {
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
            .and_then(serde_yaml::Value::as_bool)
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
            .and_then(serde_yaml::Value::as_sequence)
            .map(|seq| {
                seq.iter()
                    .filter_map(serde_yaml::Value::as_str)
                    .map(ToString::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let references = phrase_refs_from_yaml(yaml).or_else(|| {
            let function_id = yaml
                .get("function")
                .and_then(serde_yaml::Value::as_str)
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

fn phrase_filter_matches_remote(local: &serde_yaml::Value, remote: &Value) -> bool {
    let local_language_code = non_empty(
        yaml_str(local, "language_code"),
        yaml_str(local, "languageCode"),
    );
    yaml_str(local, "name") == remote.get("title").and_then(Value::as_str).unwrap_or("")
        && yaml_str(local, "description")
            == remote
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
        && yaml_bool(
            local.get("say_phrase").or_else(|| local.get("sayPhrase")),
            false,
        ) == remote
            .get("sayPhrase")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && local_language_code
            == remote
                .get("languageCode")
                .and_then(Value::as_str)
                .unwrap_or("")
        && yaml_string_list(
            local
                .get("regular_expressions")
                .or_else(|| local.get("regularExpressions")),
        ) == remote
            .get("regularExpressions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(ToString::to_string)
            .collect::<Vec<_>>()
}

fn non_empty(left: String, right: String) -> String {
    if left.is_empty() { right } else { left }
}

fn yaml_bool(value: Option<&serde_yaml::Value>, default: bool) -> bool {
    value
        .and_then(serde_yaml::Value::as_bool)
        .unwrap_or(default)
}

fn yaml_string_list(value: Option<&serde_yaml::Value>) -> Vec<String> {
    value
        .and_then(serde_yaml::Value::as_sequence)
        .map(|seq| {
            seq.iter()
                .filter_map(serde_yaml::Value::as_str)
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
/// Handoff default changes intentionally go into `post_updates`, matching the
/// Python ADK ordering where default selection is applied after handoff
/// create/update commands have established the target IDs.
pub(crate) fn interaction_aggregate_command_groups(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> CommandGroups {
    let mut ho_del = Vec::new();
    let mut ho_create = Vec::new();
    let mut ho_update = Vec::new();
    let mut sms_del = Vec::new();
    let mut sms_create = Vec::new();
    let mut sms_update = Vec::new();
    let mut sk_del = Vec::new();
    let mut sk_create = Vec::new();
    let mut sk_update = Vec::new();
    let remote_ho = remote_handoffs(projection);
    let rsms = remote_sms(projection);
    let rpf = remote_phrase_filters(projection);

    let mut local_ho_names = HashSet::new();
    let mut changed_ho_names = HashSet::new();
    let mut local_sms_names = HashSet::new();
    let mut local_pf_titles = HashSet::new();

    {
        let mut handoff_queue = HandoffItemQueue {
            projection,
            remote: &remote_ho,
            metadata,
            local_names: &mut local_ho_names,
            creates: &mut ho_create,
            updates: &mut ho_update,
        };
        let mut sms_queue = SmsItemQueue {
            projection,
            remote: &rsms,
            metadata,
            local_names: &mut local_sms_names,
            creates: &mut sms_create,
            updates: &mut sms_update,
        };
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
                .and_then(Value::as_str)
                .unwrap_or_default();

            if path == "config/handoffs.yaml" {
                if let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(content)
                    && let Some(items) = yaml
                        .get("handoffs")
                        .and_then(serde_yaml::Value::as_sequence)
                {
                    for item in items {
                        if let Some(name) = handoff_queue.queue(item, "local") {
                            changed_ho_names.insert(name);
                        }
                    }
                }
                continue;
            }

            if path.starts_with("config/handoffs.yaml/handoffs/") {
                let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(content) else {
                    continue;
                };
                if let Some(name) = handoff_queue.queue(&yaml, &resource.resource_id) {
                    changed_ho_names.insert(name);
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
                        sms_queue.queue(item, "local");
                    }
                }
                continue;
            }

            if path.starts_with("config/sms_templates.yaml/sms_templates/") {
                if let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(content) {
                    sms_queue.queue(&yaml, &resource.resource_id);
                }
                continue;
            }

            if path == "voice/response_control/phrase_filtering.yaml" {
                if let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(content)
                    && let Some(items) = yaml
                        .get("phrase_filtering")
                        .and_then(serde_yaml::Value::as_sequence)
                {
                    for item in items {
                        phrase_filter_queue.queue(item, "local");
                    }
                }
                continue;
            }

            if path.starts_with("voice/response_control/phrase_filtering.yaml/phrase_filtering/") {
                if let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(content) {
                    phrase_filter_queue.queue(&yaml, &resource.resource_id);
                }
                continue;
            }
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
        let content = resource
            .payload
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();

        if path == "config/handoffs.yaml" {
            if let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(content)
                && let Some(items) = yaml
                    .get("handoffs")
                    .and_then(serde_yaml::Value::as_sequence)
            {
                for item in items {
                    queue_handoff_default_item(
                        item,
                        "local",
                        projection,
                        &remote_ho,
                        &changed_ho_names,
                        metadata,
                        &mut defaults,
                    );
                }
            }
            continue;
        }

        if path.starts_with("config/handoffs.yaml/handoffs/") {
            let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(content) else {
                continue;
            };
            queue_handoff_default_item(
                &yaml,
                &resource.resource_id,
                projection,
                &remote_ho,
                &changed_ho_names,
                metadata,
                &mut defaults,
            );
        }
    }
    let mut groups = CommandGroups::default();
    groups.deletes.extend(ho_del);
    groups.deletes.extend(sms_del);
    groups.deletes.extend(sk_del);
    groups.creates.extend(ho_create);
    groups.creates.extend(sms_create);
    groups.creates.extend(sk_create);
    groups.updates.extend(ho_update);
    groups.updates.extend(sms_update);
    groups.updates.extend(sk_update);
    groups.post_updates = defaults;
    groups
}

pub(crate) fn payload_json_summary(payload: &CommandPayload) -> Option<(&'static str, Value)> {
    match payload {
        CommandPayload::HandoffDelete(delete) => Some((
            "handoff_delete",
            serde_json::json!({
                "id": delete.id,
            }),
        )),
        CommandPayload::HandoffCreate(create) => Some((
            "handoff_create",
            serde_json::json!({
                "id": create.id,
                "name": create.name,
                "description": create.description,
                "sip_config": sip_config_json(create.sip_config.as_ref()),
                "sip_headers": sip_headers_json(create.sip_headers.as_ref()),
                "active": create.active,
            }),
        )),
        CommandPayload::HandoffUpdate(update) => Some((
            "handoff_update",
            serde_json::json!({
                "id": update.id,
                "name": update.name.clone().unwrap_or_default(),
                "description": update.description.clone().unwrap_or_default(),
                "sip_config": sip_config_json(update.sip_config.as_ref()),
                "sip_headers": sip_headers_json(update.sip_headers.as_ref()),
                "active": update.active.unwrap_or(false),
            }),
        )),
        CommandPayload::HandoffSetDefault(update) => Some((
            "handoff_set_default",
            serde_json::json!({
                "id": update.id,
            }),
        )),
        CommandPayload::SmsDeleteTemplate(delete) => Some((
            "sms_delete_template",
            serde_json::json!({
                "id": delete.id,
            }),
        )),
        CommandPayload::SmsCreateTemplate(create) => Some((
            "sms_create_template",
            serde_json::json!({
                "id": create.id,
                "name": create.name,
                "text": create.text,
                "env_phone_numbers": sms_env_json(create.env_phone_numbers.as_ref()),
                "references": sms_references_json(create.references.as_ref()),
                "active": create.active,
            }),
        )),
        CommandPayload::SmsUpdateTemplate(update) => {
            let mut value = serde_json::Map::new();
            value.insert("id".to_string(), Value::String(update.id.clone()));
            value.insert(
                "name".to_string(),
                Value::String(update.name.clone().unwrap_or_default()),
            );
            value.insert(
                "text".to_string(),
                Value::String(update.text.clone().unwrap_or_default()),
            );
            value.insert(
                "env_phone_numbers".to_string(),
                sms_env_update_json(update.env_phone_numbers.as_ref()),
            );
            if update.references.is_some() {
                value.insert(
                    "references".to_string(),
                    sms_references_json(update.references.as_ref()),
                );
            }
            value.insert(
                "active".to_string(),
                Value::Bool(update.active.unwrap_or(false)),
            );
            Some(("sms_update_template", Value::Object(value)))
        }
        CommandPayload::StopKeywordsDelete(delete) => Some((
            "stop_keywords_delete",
            serde_json::json!({
                "id": delete.id,
            }),
        )),
        CommandPayload::StopKeywordsCreate(create) => {
            let mut value = serde_json::Map::new();
            value.insert("id".to_string(), Value::String(create.id.clone()));
            value.insert("title".to_string(), Value::String(create.title.clone()));
            value.insert(
                "description".to_string(),
                Value::String(create.description.clone()),
            );
            value.insert(
                "regular_expressions".to_string(),
                serde_json::json!(create.regular_expressions),
            );
            if create.say_phrase {
                value.insert("say_phrase".to_string(), Value::Bool(true));
            }
            value.insert(
                "references".to_string(),
                stop_keyword_references_json(create.references.as_ref()),
            );
            value.insert(
                "language_code".to_string(),
                Value::String(create.language_code.clone()),
            );
            Some(("stop_keywords_create", Value::Object(value)))
        }
        CommandPayload::StopKeywordsUpdate(update) => {
            let mut value = serde_json::Map::new();
            value.insert("id".to_string(), Value::String(update.id.clone()));
            value.insert(
                "title".to_string(),
                Value::String(update.title.clone().unwrap_or_default()),
            );
            value.insert(
                "description".to_string(),
                Value::String(update.description.clone().unwrap_or_default()),
            );
            value.insert(
                "regular_expressions".to_string(),
                serde_json::json!(update.regular_expressions),
            );
            if let Some(say_phrase) = update.say_phrase {
                value.insert("say_phrase".to_string(), Value::Bool(say_phrase));
            }
            value.insert(
                "references".to_string(),
                stop_keyword_references_json(update.references.as_ref()),
            );
            value.insert(
                "language_code".to_string(),
                Value::String(update.language_code.clone().unwrap_or_default()),
            );
            Some(("stop_keywords_update", Value::Object(value)))
        }
        _ => None,
    }
}

fn sip_config_json(config: Option<&SipConfig>) -> Value {
    match config.and_then(|config| config.config.as_ref()) {
        Some(sip_config::Config::Invite(invite)) => serde_json::json!({
            "invite": {
                "phone_number": invite.phone_number,
                "outbound_endpoint": invite.outbound_endpoint,
                "outbound_encryption": invite.outbound_encryption,
            }
        }),
        Some(sip_config::Config::Refer(refer)) => serde_json::json!({
            "refer": {
                "phone_number": refer.phone_number,
            }
        }),
        _ => serde_json::json!({ "bye": {} }),
    }
}

fn sip_headers_json(headers: Option<&SipHeaders>) -> Value {
    let Some(headers) = headers else {
        return serde_json::json!({});
    };
    if headers.headers.is_empty() {
        return serde_json::json!({});
    }
    serde_json::json!({
        "headers": headers
            .headers
            .iter()
            .map(|header| serde_json::json!({
                "key": header.key,
                "value": header.value,
            }))
            .collect::<Vec<_>>()
    })
}

fn sms_env_json(env: Option<&SmsEnvPhoneNumbers>) -> Value {
    let Some(env) = env else {
        return serde_json::json!({});
    };
    let mut value = serde_json::Map::new();
    if !env.sandbox.is_empty() {
        value.insert("sandbox".to_string(), Value::String(env.sandbox.clone()));
    }
    if !env.pre_release.is_empty() {
        value.insert(
            "pre_release".to_string(),
            Value::String(env.pre_release.clone()),
        );
    }
    if !env.live.is_empty() {
        value.insert("live".to_string(), Value::String(env.live.clone()));
    }
    Value::Object(value)
}

fn sms_env_update_json(env: Option<&UpdateSmsEnvPhoneNumbers>) -> Value {
    let Some(env) = env else {
        return serde_json::json!({});
    };
    let mut value = serde_json::Map::new();
    if let Some(sandbox) = &env.sandbox {
        value.insert("sandbox".to_string(), Value::String(sandbox.clone()));
    }
    if let Some(pre_release) = &env.pre_release {
        value.insert(
            "pre_release".to_string(),
            Value::String(pre_release.clone()),
        );
    }
    if let Some(live) = &env.live {
        value.insert("live".to_string(), Value::String(live.clone()));
    }
    Value::Object(value)
}

fn sms_references_json(references: Option<&SmsTemplateReferences>) -> Value {
    let Some(references) = references else {
        return serde_json::json!({});
    };
    let mut value = serde_json::Map::new();
    if !references.topics.is_empty() {
        value.insert("topics".to_string(), serde_json::json!(references.topics));
    }
    if !references.flow_steps.is_empty() {
        value.insert(
            "flow_steps".to_string(),
            serde_json::json!(references.flow_steps),
        );
    }
    if !references.variables.is_empty() {
        value.insert(
            "variables".to_string(),
            serde_json::json!(references.variables),
        );
    }
    if !references.translations.is_empty() {
        value.insert(
            "translations".to_string(),
            serde_json::json!(references.translations),
        );
    }
    Value::Object(value)
}

fn stop_keyword_references_json(references: Option<&StopKeywordReferences>) -> Value {
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
#[path = "interactions_tests.rs"]
mod tests;
