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
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub(crate) fn handoff_command_groups(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> CommandGroups {
    let remote = remote_handoffs(projection);
    let mut deletes = Vec::new();
    let mut creates = Vec::new();
    let mut updates = Vec::new();
    let mut local_names = HashSet::new();
    let mut changed_names = HashSet::new();

    {
        let mut queue = HandoffItemQueue {
            projection,
            remote: &remote,
            metadata,
            local_names: &mut local_names,
            creates: &mut creates,
            updates: &mut updates,
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
                        if let Some(name) = queue.queue(item, "local") {
                            changed_names.insert(name);
                        }
                    }
                }
                continue;
            }

            if path.starts_with("config/handoffs.yaml/handoffs/") {
                let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(content) else {
                    continue;
                };
                if let Some(name) = queue.queue(&yaml, &resource.resource_id) {
                    changed_names.insert(name);
                }
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
                        &remote,
                        &changed_names,
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
                &remote,
                &changed_names,
                metadata,
                &mut defaults,
            );
        }
    }

    CommandGroups {
        deletes,
        creates,
        updates,
        post_updates: defaults,
    }
}

fn remote_handoffs(projection: &Value) -> HashMap<String, String> {
    let entities = extract_entities_map(projection, &["handoff", "handoffs", "entities"]);
    let mut handoffs = HashMap::new();
    for (id, value) in entities {
        if !value.get("active").and_then(Value::as_bool).unwrap_or(true) {
            continue;
        }
        let name = value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&id)
            .to_string();
        handoffs.insert(name, id);
    }
    handoffs
}

fn yaml_str(yaml: &serde_yaml::Value, key: &str) -> String {
    yaml.get(key)
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn json_str(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn handoff_sip_config(yaml: &serde_yaml::Value) -> SipConfig {
    let config = match yaml.get("sip_config").or_else(|| yaml.get("sipConfig")) {
        Some(value) => value,
        None => {
            return SipConfig {
                config: Some(sip_config::Config::Bye(SipByeHandoffConfig {})),
            };
        }
    };
    let method = config
        .get("method")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or("bye");
    let config = match method {
        "invite" => sip_config::Config::Invite(SipInviteHandoffConfig {
            phone_number: yaml_str(config, "phone_number"),
            outbound_endpoint: yaml_str(config, "outbound_endpoint"),
            outbound_encryption: yaml_str(config, "outbound_encryption"),
        }),
        "refer" => sip_config::Config::Refer(SipReferHandoffConfig {
            phone_number: yaml_str(config, "phone_number"),
        }),
        _ => sip_config::Config::Bye(SipByeHandoffConfig {}),
    };
    SipConfig {
        config: Some(config),
    }
}

fn sip_headers_from_yaml(yaml: &serde_yaml::Value) -> Option<SipHeaders> {
    let headers = yaml
        .get("sip_headers")
        .or_else(|| yaml.get("sipHeaders"))?
        .as_sequence()?;
    let headers = headers
        .iter()
        .filter_map(|row| {
            Some(SipHeader {
                key: row.get("key")?.as_str()?.to_string(),
                value: row.get("value")?.as_str()?.to_string(),
            })
        })
        .collect::<Vec<_>>();
    (!headers.is_empty()).then_some(SipHeaders { headers })
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

fn queue_handoff_default_item(
    yaml: &serde_yaml::Value,
    resource_id: &str,
    projection: &Value,
    remote_handoffs: &HashMap<String, String>,
    changed_names: &HashSet<String>,
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
    if let Some(remote_id) = remote_handoffs.get(&name)
        && let Some(remote) = extract_entities_map(projection, &["handoff", "handoffs", "entities"])
            .get(remote_id)
            .cloned()
        && remote
            .get("isDefault")
            .or_else(|| remote.get("is_default"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && !changed_names.contains(&name)
    {
        return;
    }
    let id = remote_handoffs
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
    (!headers.is_empty()).then_some(SipHeaders { headers })
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

    fn flatten(groups: CommandGroups) -> Vec<Command> {
        groups
            .deletes
            .into_iter()
            .chain(groups.creates)
            .chain(groups.updates)
            .chain(groups.post_updates)
            .collect()
    }

    #[test]
    fn handoff_create_and_set_default() {
        let handoff_yaml = r#"
name: Sales
description: "to sales"
is_default: true
sip_config:
  method: bye
sip_headers: []
"#;
        let resources = map_with(vec![(
            "config/handoffs.yaml/handoffs/Sales".into(),
            Resource {
                resource_id: "local".into(),
                name: "Sales".into(),
                file_path: "config/handoffs.yaml/handoffs/Sales".into(),
                payload: serde_json::json!({ "content": handoff_yaml }),
            },
        )]);
        let projection = serde_json::json!({});
        let commands = flatten(handoff_command_groups(&resources, &projection, &None));
        let types: Vec<&str> = commands.iter().map(|c| c.r#type.as_str()).collect();
        assert!(types.contains(&"handoff_create"));
        assert!(types.contains(&"handoff_set_default"));
    }

    #[test]
    fn remote_handoff_without_active_field_is_treated_as_active() {
        let resources = map_with(vec![(
            "config/handoffs.yaml/handoffs/Sales".into(),
            Resource {
                resource_id: "ho-sales".into(),
                name: "Sales".into(),
                file_path: "config/handoffs.yaml/handoffs/Sales".into(),
                payload: serde_json::json!({
                    "content": "name: Sales\ndescription: updated sales route\n"
                }),
            },
        )]);
        let projection = serde_json::json!({
            "handoff": {
                "handoffs": {
                    "entities": {
                        "ho-sales": {
                            "name": "Sales",
                            "description": "to sales"
                        }
                    }
                }
            }
        });
        let commands = flatten(handoff_command_groups(&resources, &projection, &None));
        assert!(
            !commands
                .iter()
                .any(|command| command.r#type == "handoff_create"),
            "existing active-by-default handoff should not be recreated"
        );
        assert!(
            commands
                .iter()
                .any(|command| command.r#type == "handoff_update"),
            "existing active-by-default handoff should be updated if needed"
        );
    }
}
