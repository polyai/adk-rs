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
fn handoff_payload_summaries_cover_sip_shapes_and_defaults() {
    let payloads = [
        (
            CommandPayload::HandoffCreate(HandoffCreate {
                id: "ho-1".into(),
                name: "Sales".into(),
                description: "transfer".into(),
                sip_config: Some(SipConfig {
                    config: Some(sip_config::Config::Invite(SipInviteHandoffConfig {
                        phone_number: "+1555".into(),
                        outbound_endpoint: "sip.example.com".into(),
                        outbound_encryption: "tls".into(),
                    })),
                }),
                sip_headers: Some(SipHeaders {
                    headers: vec![SipHeader {
                        key: "X-Team".into(),
                        value: "sales".into(),
                    }],
                }),
                active: true,
                references: None,
            }),
            "handoff_create",
            serde_json::json!({
                "id": "ho-1",
                "name": "Sales",
                "description": "transfer",
                "sip_config": {"invite": {"phone_number": "+1555", "outbound_endpoint": "sip.example.com", "outbound_encryption": "tls"}},
                "sip_headers": {"headers": [{"key": "X-Team", "value": "sales"}]},
                "active": true,
            }),
        ),
        (
            CommandPayload::HandoffUpdate(HandoffUpdate {
                id: "ho-2".into(),
                name: None,
                description: Some("refer".into()),
                sip_config: Some(SipConfig {
                    config: Some(sip_config::Config::Refer(SipReferHandoffConfig {
                        phone_number: "+1666".into(),
                    })),
                }),
                sip_headers: None,
                active: None,
                references: None,
            }),
            "handoff_update",
            serde_json::json!({
                "id": "ho-2",
                "name": "",
                "description": "refer",
                "sip_config": {"refer": {"phone_number": "+1666"}},
                "sip_headers": {},
                "active": false,
            }),
        ),
        (
            CommandPayload::HandoffDelete(HandoffDelete { id: "ho-3".into() }),
            "handoff_delete",
            serde_json::json!({"id": "ho-3"}),
        ),
        (
            CommandPayload::HandoffSetDefault(HandoffSetDefault { id: "ho-4".into() }),
            "handoff_set_default",
            serde_json::json!({"id": "ho-4"}),
        ),
    ];

    for (payload, key, value) in payloads {
        assert_eq!(payload_json_summary(&payload), Some((key, value)));
    }
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
