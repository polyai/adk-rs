use super::*;
use crate::build_push_commands;
use crate::ids::stable_resource_id;
use crate::test_support::local_resource;

fn yaml(value: &str) -> YamlValue {
    serde_yaml_ng::from_str(value).expect("valid entity config yaml")
}

#[test]
fn entity_config_builders_and_json_cover_supported_types() {
    let cases = [
        (
            "numeric",
            "has_decimal: true\nhas_range: true\nmin: 1.5\nmax: 9\n",
            serde_json::json!({"has_decimal": true, "has_range": true, "min": 1.5, "max": 9.0}),
        ),
        (
            "alphanumeric",
            "enabled: false\nvalidation_type: regex\nregular_expression: '^[A-Z]+$'\n",
            serde_json::json!({"enabled": false, "validation_type": "regex", "regular_expression": "^[A-Z]+$"}),
        ),
        (
            "enum",
            "options: [small, large]\n",
            serde_json::json!({"options": ["small", "large"]}),
        ),
        (
            "date",
            "relative_date: true\n",
            serde_json::json!({"relative_date": true}),
        ),
        (
            "phone_number",
            "enabled: false\ncountry_codes: ['+1', '+44']\n",
            serde_json::json!({"enabled": false, "country_codes": ["+1", "+44"]}),
        ),
        (
            "time",
            "enabled: false\nstart_time: '09:00'\nend_time: '17:00'\n",
            serde_json::json!({"enabled": false, "start_time": "09:00", "end_time": "17:00"}),
        ),
        ("address", "{}\n", serde_json::json!({})),
        ("free_text", "{}\n", serde_json::json!({})),
        ("name_config", "{}\n", serde_json::json!({})),
    ];

    for (entity_type, source, expected) in cases {
        let config = yaml(source);
        let create = build_entity_create_config(entity_type, Some(&config));
        let update = build_entity_update_config(entity_type, Some(&config));

        assert_eq!(
            entity_create_config_json(create.as_ref()),
            Some((entity_type, expected.clone())),
            "create config json for {entity_type}"
        );
        assert_eq!(
            entity_update_config_json(update.as_ref()),
            Some((entity_type, expected)),
            "update config json for {entity_type}"
        );
    }

    assert!(build_entity_create_config("unknown", None).is_none());
    assert!(build_entity_update_config("unknown", None).is_none());
    assert!(entity_create_config_json(None).is_none());
    assert!(entity_update_config_json(None).is_none());
}

#[test]
fn entity_payload_summaries_include_selected_config_shapes() {
    let payloads = [
        (
            CommandPayload::EntityCreate(EntityCreate {
                id: "ent-1".into(),
                name: "Size".into(),
                r#type: "Enum".into(),
                description: "shirt size".into(),
                references: None,
                config: Some(entities::entity_create::Config::Enum(
                    entities::MultipleOptionsConfig {
                        options: vec!["S".into(), "M".into()],
                    },
                )),
            }),
            "entity_create",
            serde_json::json!({
                "id": "ent-1",
                "name": "Size",
                "type": "Enum",
                "description": "shirt size",
                "references": {},
                "enum": {"options": ["S", "M"]},
            }),
        ),
        (
            CommandPayload::EntityUpdate(EntityUpdate {
                id: "ent-2".into(),
                name: "When".into(),
                r#type: "Time".into(),
                description: "appointment".into(),
                references: None,
                config: Some(entities::entity_update::Config::Time(
                    entities::TimeConfig {
                        enabled: true,
                        start_time: "08:00".into(),
                        end_time: "18:00".into(),
                    },
                )),
            }),
            "entity_update",
            serde_json::json!({
                "id": "ent-2",
                "name": "When",
                "type": "Time",
                "description": "appointment",
                "time": {"enabled": true, "start_time": "08:00", "end_time": "18:00"},
            }),
        ),
        (
            CommandPayload::EntityDelete(EntityDelete { id: "ent-3".into() }),
            "entity_delete",
            serde_json::json!({"id": "ent-3"}),
        ),
    ];

    for (payload, expected_key, expected_json) in payloads {
        assert_eq!(
            payload_json_summary(&payload),
            Some((expected_key, expected_json))
        );
    }
}

#[test]
fn entity_create_populates_flow_reverse_references() {
    let mut resources = ResourceMap::new();
    resources.insert(
        "config/entities.yaml".to_string(),
        local_resource(
            "config/entities.yaml",
            "entities",
            "entities:\n  - name: customer_id\n    description: Customer id\n    entity_type: free_text\n    config: {}\n  - name: confirmation\n    description: Confirmation\n    entity_type: enum\n    config:\n      options: [yes, no]\n",
        ),
    );
    resources.insert(
        "flows/support/flow_config.yaml".to_string(),
        local_resource(
            "flows/support/flow_config.yaml",
            "support",
            "name: support\ndescription: Support flow\nstart_step: collect\n",
        ),
    );
    resources.insert(
        "flows/support/steps/collect.yaml".to_string(),
        local_resource(
            "flows/support/steps/collect.yaml",
            "collect",
            "step_type: default_step\nname: collect\nprompt: Collect {{entity:customer_id}}\nconditions:\n  - name: done\n    condition_type: exit_flow_condition\n    description: Done.\n    required_entities:\n      - customer_id\nextracted_entities:\n  - customer_id\n",
        ),
    );
    resources.insert(
        "flows/support/steps/confirm.yaml".to_string(),
        local_resource(
            "flows/support/steps/confirm.yaml",
            "confirm",
            "step_type: advanced_step\nname: confirm\nprompt: Confirm {{entity:confirmation}}\n",
        ),
    );

    let commands = build_push_commands(&resources, &serde_json::json!({}));
    let collect_step_id =
        stable_resource_id("FLOW_STEPS", "collect", "flows/support/steps/collect.yaml");
    let confirm_step_id =
        stable_resource_id("FLOW_STEPS", "confirm", "flows/support/steps/confirm.yaml");
    let customer_id = stable_resource_id(ENTITY_ID_PREFIX, "customer_id", ENTITIES_FILE.file_path);
    let confirmation_id =
        stable_resource_id(ENTITY_ID_PREFIX, "confirmation", ENTITIES_FILE.file_path);

    let customer_create = commands
        .iter()
        .find_map(|command| match &command.payload {
            Some(CommandPayload::EntityCreate(create)) if create.id == customer_id => Some(create),
            _ => None,
        })
        .expect("customer_id create");
    assert_eq!(
        customer_create
            .references
            .as_ref()
            .expect("customer_id references")
            .no_code_steps
            .get(&collect_step_id),
        Some(&true)
    );

    let confirmation_create = commands
        .iter()
        .find_map(|command| match &command.payload {
            Some(CommandPayload::EntityCreate(create)) if create.id == confirmation_id => {
                Some(create)
            }
            _ => None,
        })
        .expect("confirmation create");
    assert_eq!(
        confirmation_create
            .references
            .as_ref()
            .expect("confirmation references")
            .flow_steps
            .get(&confirm_step_id),
        Some(&true)
    );
}
