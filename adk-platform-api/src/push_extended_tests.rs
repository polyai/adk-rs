use super::*;
use adk_domain::Resource;
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
fn variable_create_and_delete_roundtrip_types() {
    let mut resources = map_with(vec![(
        "variables/OrderId".into(),
        Resource {
            resource_id: "local".into(),
            name: "OrderId".into(),
            file_path: "variables/OrderId".into(),
            payload: serde_json::json!({ "content": "" }),
        },
    )]);
    let projection = serde_json::json!({});
    let commands = flatten(extended_resource_command_groups(
        &resources,
        &projection,
        &None,
    ));
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].r#type, "variable_create");
    assert!(matches!(
        commands[0].payload,
        Some(CommandPayload::VariableCreate(_))
    ));

    resources.clear();
    let projection = serde_json::json!({
        "variables": { "variables": { "entities": {
            "vrbl-x": { "name": "OrderId" }
        }}}
    });
    let commands = flatten(extended_resource_command_groups(
        &resources,
        &projection,
        &None,
    ));
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].r#type, "variable_delete");
}

#[test]
fn handoff_create_set_default_and_sms_create() {
    let ho_yaml = r#"
name: Sales
description: "to sales"
is_default: true
sip_config:
  method: bye
sip_headers: []
"#;
    let sms_yaml = r#"
name: Welcome
text: hi {{var}}
env_phone_numbers:
  sandbox: "+100"
  pre_release: "+200"
  live: "+300"
"#;
    let resources = map_with(vec![
        (
            "config/handoffs.yaml/handoffs/Sales".into(),
            Resource {
                resource_id: "local".into(),
                name: "Sales".into(),
                file_path: "config/handoffs.yaml/handoffs/Sales".into(),
                payload: serde_json::json!({ "content": ho_yaml }),
            },
        ),
        (
            "config/sms_templates.yaml/sms_templates/Welcome".into(),
            Resource {
                resource_id: "local".into(),
                name: "Welcome".into(),
                file_path: "config/sms_templates.yaml/sms_templates/Welcome".into(),
                payload: serde_json::json!({ "content": sms_yaml }),
            },
        ),
    ]);
    let projection = serde_json::json!({});
    let commands = flatten(extended_resource_command_groups(
        &resources,
        &projection,
        &None,
    ));
    let types: Vec<&str> = commands.iter().map(|c| c.r#type.as_str()).collect();
    assert!(types.contains(&"handoff_create"));
    assert!(types.contains(&"handoff_set_default"));
    assert!(types.contains(&"sms_create_template"));
}

#[test]
fn remote_handoff_without_active_field_is_treated_as_active() {
    let mut resources = ResourceMap::new();
    resources.insert(
        "config/handoffs.yaml/handoffs/Sales".into(),
        Resource {
            resource_id: "ho-sales".into(),
            name: "Sales".into(),
            file_path: "config/handoffs.yaml/handoffs/Sales".into(),
            payload: serde_json::json!({
                "content": "name: Sales\ndescription: to sales\n"
            }),
        },
    );
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
    let commands = flatten(extended_resource_command_groups(
        &resources,
        &projection,
        &None,
    ));
    assert!(
        !commands.iter().any(|c| c.r#type == "handoff_create"),
        "existing active-by-default handoff should not be recreated"
    );
    assert!(
        commands.iter().any(|c| c.r#type == "handoff_update"),
        "existing active-by-default handoff should be updated if needed"
    );
}

#[test]
fn sms_create_populates_references_from_yaml() {
    let sms_yaml = r#"
name: Welcome
text: hi
references:
  topics:
    topic-1: true
  flow_steps:
    flow-1: true
  variables:
    var-1: true
  translations:
    tr-1: true
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
    let projection = serde_json::json!({});
    let commands = flatten(extended_resource_command_groups(
        &resources,
        &projection,
        &None,
    ));
    let create = commands
        .iter()
        .find(|c| c.r#type == "sms_create_template")
        .expect("sms create command");
    match &create.payload {
        Some(CommandPayload::SmsCreateTemplate(msg)) => {
            let refs = msg.references.as_ref().expect("references");
            assert!(refs.topics.get("topic-1").copied().unwrap_or(false));
            assert!(refs.flow_steps.get("flow-1").copied().unwrap_or(false));
            assert!(refs.variables.get("var-1").copied().unwrap_or(false));
            assert!(refs.translations.get("tr-1").copied().unwrap_or(false));
        }
        other => panic!("unexpected payload: {other:?}"),
    }
}

#[test]
fn stop_keyword_references_include_global_functions_map() {
    let pf_yaml = r#"
name: HangUp
description: end
regular_expressions:
  - "^bye$"
say_phrase: false
language_code: en-US
references:
  global_functions:
    fn-one: true
    fn-two: false
"#;
    let resources = map_with(vec![(
        "voice/response_control/phrase_filtering.yaml/phrase_filtering/HangUp".into(),
        Resource {
            resource_id: "sk-hangup".into(),
            name: "HangUp".into(),
            file_path: "voice/response_control/phrase_filtering.yaml/phrase_filtering/HangUp"
                .into(),
            payload: serde_json::json!({ "content": pf_yaml }),
        },
    )]);
    let projection = serde_json::json!({});
    let commands = flatten(extended_resource_command_groups(
        &resources,
        &projection,
        &None,
    ));
    let create = commands
        .iter()
        .find(|c| c.r#type == "stop_keywords_create")
        .expect("stop keyword create command");
    match &create.payload {
        Some(CommandPayload::StopKeywordsCreate(msg)) => {
            let refs = msg.references.as_ref().expect("references");
            assert_eq!(refs.global_functions.get("fn-one"), Some(&true));
            assert_eq!(refs.global_functions.get("fn-two"), Some(&false));
        }
        other => panic!("unexpected payload: {other:?}"),
    }
}

#[test]
fn stop_keywords_create_and_experimental_update() {
    let pf_yaml = r#"
name: HangUp
description: end
regular_expressions:
  - "^bye$"
say_phrase: false
language_code: en-US
"#;
    let resources = map_with(vec![
        (
            "voice/response_control/phrase_filtering.yaml/phrase_filtering/HangUp".into(),
            Resource {
                resource_id: "local".into(),
                name: "HangUp".into(),
                file_path: "voice/response_control/phrase_filtering.yaml/phrase_filtering/HangUp"
                    .into(),
                payload: serde_json::json!({ "content": pf_yaml }),
            },
        ),
        (
            "agent_settings/experimental_config.json".into(),
            Resource {
                resource_id: "default".into(),
                name: "experimental_config".into(),
                file_path: "agent_settings/experimental_config.json".into(),
                payload: serde_json::json!({
                    "content": r#"{ "flag_test": true }"#
                }),
            },
        ),
    ]);
    let projection = serde_json::json!({});
    let commands = flatten(extended_resource_command_groups(
        &resources,
        &projection,
        &None,
    ));
    let types: Vec<&str> = commands.iter().map(|c| c.r#type.as_str()).collect();
    assert!(types.contains(&"stop_keywords_create"));
    assert!(types.contains(&"experimental_config_update_config"));
    let exp_cmd = commands
        .iter()
        .find(|c| c.r#type == "experimental_config_update_config")
        .expect("experimental config update command");
    match &exp_cmd.payload {
        Some(CommandPayload::ExperimentalConfigUpdateConfig(msg)) => {
            assert_eq!(msg.id, "default")
        }
        other => panic!("unexpected payload: {other:?}"),
    }
}
