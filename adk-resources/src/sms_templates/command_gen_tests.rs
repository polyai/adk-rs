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
        .chain(groups.cleanup_deletes)
        .chain(groups.post_deletes)
        .collect()
}

#[test]
fn sms_payload_summaries_cover_env_refs_and_update_defaults() {
    let mut topics = HashMap::new();
    topics.insert("topic-1".to_string(), true);
    let mut variables = HashMap::new();
    variables.insert("var-1".to_string(), false);
    let payloads = [
        (
            CommandPayload::SmsCreateTemplate(SmsCreateTemplate {
                id: "sms-1".into(),
                name: "Welcome".into(),
                text: "Hi".into(),
                env_phone_numbers: Some(SmsEnvPhoneNumbers {
                    sandbox: "+100".into(),
                    pre_release: "".into(),
                    live: "+199".into(),
                }),
                references: Some(SmsTemplateReferences {
                    topics,
                    flow_steps: HashMap::new(),
                    variables,
                    translations: HashMap::new(),
                }),
                active: true,
            }),
            "sms_create_template",
            serde_json::json!({
                "id": "sms-1",
                "name": "Welcome",
                "text": "Hi",
                "env_phone_numbers": {"sandbox": "+100", "live": "+199"},
                "references": {"topics": {"topic-1": true}, "variables": {"var-1": false}},
                "active": true,
            }),
        ),
        (
            CommandPayload::SmsUpdateTemplate(SmsUpdateTemplate {
                id: "sms-2".into(),
                name: None,
                text: Some("Updated".into()),
                env_phone_numbers: Some(UpdateSmsEnvPhoneNumbers {
                    sandbox: Some("+200".into()),
                    pre_release: None,
                    live: Some("+299".into()),
                }),
                references: None,
                active: None,
            }),
            "sms_update_template",
            serde_json::json!({
                "id": "sms-2",
                "name": "",
                "text": "Updated",
                "env_phone_numbers": {"sandbox": "+200", "live": "+299"},
                "active": false,
            }),
        ),
        (
            CommandPayload::SmsDeleteTemplate(SmsDeleteTemplate { id: "sms-3".into() }),
            "sms_delete_template",
            serde_json::json!({"id": "sms-3"}),
        ),
    ];

    for (payload, key, value) in payloads {
        assert_eq!(payload_json_summary(&payload), Some((key, value)));
    }
}

#[test]
fn sms_create_derives_references_from_inline_text() {
    let sms_yaml = r#"
name: Welcome
text: hi {{vrbl:customer_name}} {{var:customer_alias}} {{tr:greeting}}
env_phone_numbers:
  sandbox: ""
  pre_release: ""
  live: ""
"#;
    let resources = map_with(vec![(
        "config/sms_templates.yaml/sms_templates/Welcome".into(),
        Resource {
            resource_id: "twilio_sms-1".into(),
            name: "Welcome".into(),
            file_path: "config/sms_templates.yaml/sms_templates/Welcome".into(),
            payload: serde_json::json!({ "content": sms_yaml }),
        },
    )]);
    let projection = serde_json::json!({
        "variables": {
            "variables": {
                "entities": {
                    "VARIABLE-customer_name": {
                        "id": "VARIABLE-customer_name",
                        "name": "customer_name"
                    },
                    "VARIABLE-customer_alias": {
                        "id": "VARIABLE-customer_alias",
                        "name": "customer_alias"
                    }
                }
            }
        },
        "translations": {
            "translations": {
                "entities": {
                    "TRANSLATION-greeting": {
                        "id": "TRANSLATION-greeting",
                        "translationKey": "greeting"
                    }
                }
            }
        }
    });
    let commands = flatten(sms_template_command_groups(&resources, &projection, &None));
    let create = commands
        .iter()
        .find(|command| command.r#type == "sms_create_template")
        .expect("sms create command");
    match &create.payload {
        Some(CommandPayload::SmsCreateTemplate(message)) => {
            assert_eq!(
                message.text,
                "hi {{vrbl:VARIABLE-customer_name}} {{var:VARIABLE-customer_alias}} {{tr:TRANSLATION-greeting}}"
            );
            let refs = message.references.as_ref().expect("references");
            assert!(refs.topics.is_empty());
            assert!(refs.flow_steps.is_empty());
            assert!(
                refs.variables
                    .get("VARIABLE-customer_name")
                    .copied()
                    .unwrap_or(false)
            );
            assert!(
                refs.variables
                    .get("VARIABLE-customer_alias")
                    .copied()
                    .unwrap_or(false)
            );
            assert!(
                refs.translations
                    .get("TRANSLATION-greeting")
                    .copied()
                    .unwrap_or(false)
            );
        }
        _ => panic!("unexpected payload variant for SMS create command"),
    }
}
