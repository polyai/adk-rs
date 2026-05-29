use super::*;

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
