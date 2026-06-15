use crate::handoffs::local::{
    HANDOFF_ITEM_PREFIX, HANDOFFS_FILE_PATH, Handoff, normalize_invite_encryption,
    parse_handoffs_content,
};
use crate::ids::stable_resource_id;
use crate::push_commands::CommandGroups;
use crate::{extract_entities_map, is_synthetic_local_resource_id, push_command};
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::handoff::{
    HandoffCreate, HandoffDelete, HandoffSetDefault, HandoffUpdate, SipByeHandoffConfig, SipConfig,
    SipHeader, SipHeaders, SipInviteHandoffConfig, SipReferHandoffConfig, sip_config,
};
use adk_protobuf::{Command, Metadata};
use adk_types::ResourceMap;
use serde_json::{self, Value as JsonValue};
use std::collections::{HashMap, HashSet};

pub(crate) fn handoff_command_groups(
    resources: &ResourceMap,
    projection: &JsonValue,
    metadata: &Option<Metadata>,
) -> CommandGroups {
    let remote = remote_handoffs(projection);
    let mut deletes = Vec::new();
    let mut creates = Vec::new();
    let mut updates = Vec::new();
    let mut local_names = HashSet::new();
    let mut changed_names = HashSet::new();
    let Some(local_handoffs) = local_handoff_resources(resources) else {
        return CommandGroups::default();
    };

    {
        let mut builder = HandoffCommandBuilder {
            projection,
            remote: &remote,
            metadata,
            local_names: &mut local_names,
            creates: &mut creates,
            updates: &mut updates,
        };

        for local in &local_handoffs {
            if let Some(name) = builder.append_item(&local.handoff, &local.resource_id) {
                changed_names.insert(name);
            }
        }
    }

    for (name, id) in &remote {
        if !local_names.contains(name) {
            push_command(
                &mut deletes,
                metadata,
                "handoff_delete",
                CommandPayload::HandoffDelete(HandoffDelete { id: id.clone() }),
            );
        }
    }

    let mut defaults: Vec<Command> = Vec::new();
    for local in &local_handoffs {
        queue_handoff_default_item(
            &local.handoff,
            &local.resource_id,
            projection,
            &remote,
            &changed_names,
            metadata,
            &mut defaults,
        );
    }

    CommandGroups {
        deletes,
        creates,
        updates,
        post_updates: defaults,
        cleanup_deletes: Vec::new(),
        post_deletes: Vec::new(),
    }
}

struct LocalHandoffResource {
    resource_id: String,
    handoff: Handoff,
}

fn local_handoff_resources(resources: &ResourceMap) -> Option<Vec<LocalHandoffResource>> {
    let mut handoffs = Vec::new();
    for resource in resources.values() {
        let path = resource.file_path.as_str();
        if path != HANDOFFS_FILE_PATH && !path.starts_with(HANDOFF_ITEM_PREFIX) {
            continue;
        }
        let content = resource
            .payload
            .get("content")
            .and_then(JsonValue::as_str)
            .unwrap_or_default();
        let parsed_handoffs = parse_handoffs_content(path, content).ok()?;
        let resource_id = if path == HANDOFFS_FILE_PATH {
            "local"
        } else {
            resource.resource_id.as_str()
        };
        handoffs.extend(
            parsed_handoffs
                .into_iter()
                .map(|handoff| LocalHandoffResource {
                    resource_id: resource_id.to_string(),
                    handoff,
                }),
        );
    }
    Some(handoffs)
}

fn remote_handoffs(projection: &JsonValue) -> HashMap<String, String> {
    let entities = extract_entities_map(projection, &["handoff", "handoffs", "entities"]);
    let mut handoffs = HashMap::new();
    for (id, value) in entities {
        if !value
            .get("active")
            .and_then(JsonValue::as_bool)
            .unwrap_or(true)
        {
            continue;
        }
        let name = value
            .get("name")
            .and_then(JsonValue::as_str)
            .unwrap_or(&id)
            .to_string();
        handoffs.insert(name, id);
    }
    handoffs
}

fn json_str(value: &JsonValue, key: &str) -> String {
    value
        .get(key)
        .and_then(JsonValue::as_str)
        .unwrap_or("")
        .to_string()
}

fn invite_encryption_from_remote(value: &JsonValue) -> String {
    let encryption = json_str(value, "outboundEncryption");
    normalize_invite_encryption(&encryption).unwrap_or(encryption)
}

struct HandoffCommandBuilder<'a> {
    projection: &'a JsonValue,
    remote: &'a HashMap<String, String>,
    metadata: &'a Option<Metadata>,
    local_names: &'a mut HashSet<String>,
    creates: &'a mut Vec<Command>,
    updates: &'a mut Vec<Command>,
}

impl HandoffCommandBuilder<'_> {
    fn append_item(&mut self, handoff: &Handoff, resource_id: &str) -> Option<String> {
        let name = handoff.name().to_string();
        self.local_names.insert(name.clone());
        let id = self
            .remote
            .get(&name)
            .cloned()
            .or_else(|| {
                (!is_synthetic_local_resource_id(resource_id)).then_some(resource_id.to_string())
            })
            .unwrap_or_else(|| stable_resource_id("HANDOFFS", &name, HANDOFFS_FILE_PATH));
        let description = handoff.description().to_string();
        let sip_config = handoff.sip_config_proto();
        let sip_headers = handoff.sip_headers_proto();
        if self.remote.contains_key(&name) {
            if let Some(remote) = self.remote.get(&name).and_then(|id| {
                extract_entities_map(self.projection, &["handoff", "handoffs", "entities"])
                    .get(id)
                    .cloned()
            }) && handoff_matches_remote(handoff, &remote)
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

fn queue_handoff_default_item(
    handoff: &Handoff,
    resource_id: &str,
    projection: &JsonValue,
    remote_handoffs: &HashMap<String, String>,
    changed_names: &HashSet<String>,
    metadata: &Option<Metadata>,
    defaults: &mut Vec<Command>,
) {
    if !handoff.is_default() {
        return;
    }
    let name = handoff.name();
    if let Some(remote_id) = remote_handoffs.get(name)
        && let Some(remote) = extract_entities_map(projection, &["handoff", "handoffs", "entities"])
            .get(remote_id)
            .cloned()
        && remote
            .get("isDefault")
            .or_else(|| remote.get("is_default"))
            .and_then(JsonValue::as_bool)
            .unwrap_or(false)
        && !changed_names.contains(name)
    {
        return;
    }
    let id = remote_handoffs
        .get(name)
        .cloned()
        .or_else(|| {
            (!is_synthetic_local_resource_id(resource_id)).then_some(resource_id.to_string())
        })
        .unwrap_or_else(|| stable_resource_id("HANDOFFS", name, HANDOFFS_FILE_PATH));
    push_command(
        defaults,
        metadata,
        "handoff_set_default",
        CommandPayload::HandoffSetDefault(HandoffSetDefault { id }),
    );
}

fn handoff_matches_remote(local: &Handoff, remote: &JsonValue) -> bool {
    local.name() == remote.get("name").and_then(JsonValue::as_str).unwrap_or("")
        && local.description()
            == remote
                .get("description")
                .and_then(JsonValue::as_str)
                .unwrap_or("")
        && local.sip_config_proto() == handoff_sip_config_from_remote(remote)
        && local.sip_headers_proto() == sip_headers_from_remote(remote)
}

fn handoff_sip_config_from_remote(remote: &JsonValue) -> SipConfig {
    let Some(config) = remote
        .get("sipConfig")
        .and_then(|value| value.get("config").or(Some(value)))
    else {
        return SipConfig {
            config: Some(sip_config::Config::Bye(SipByeHandoffConfig {})),
        };
    };
    if let Some(case) = config.get("$case").and_then(JsonValue::as_str) {
        let value = config.get("value").unwrap_or(&JsonValue::Null);
        let config = match case {
            "invite" => sip_config::Config::Invite(SipInviteHandoffConfig {
                phone_number: json_str(value, "phoneNumber"),
                outbound_endpoint: json_str(value, "outboundEndpoint"),
                outbound_encryption: invite_encryption_from_remote(value),
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
                outbound_encryption: invite_encryption_from_remote(invite),
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

fn sip_headers_from_remote(remote: &JsonValue) -> Option<SipHeaders> {
    let headers = remote
        .get("sipHeaders")
        .and_then(|value| value.get("headers").or(Some(value)))
        .and_then(JsonValue::as_array)?;
    let headers = headers
        .iter()
        .filter_map(|header| {
            Some(SipHeader {
                key: header.get("key")?.as_str()?.to_string(),
                value: header.get("value")?.as_str()?.to_string(),
            })
        })
        .collect::<Vec<_>>();
    (!headers.is_empty()).then_some(SipHeaders { headers })
}

pub(crate) fn payload_json_summary(payload: &CommandPayload) -> Option<(&'static str, JsonValue)> {
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
        _ => None,
    }
}

fn sip_config_json(config: Option<&SipConfig>) -> JsonValue {
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

fn sip_headers_json(headers: Option<&SipHeaders>) -> JsonValue {
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

#[cfg(test)]
#[path = "command_gen_tests.rs"]
mod command_gen_tests;
