use super::*;
use adk_types::Resource;

#[test]
fn prost_value_json_preserves_nested_struct_and_list_shapes() {
    use prost_types::{ListValue, Struct, Value as ProstValue, value::Kind};
    use std::collections::BTreeMap;

    let nested = ProstValue {
        kind: Some(Kind::StructValue(Struct {
            fields: BTreeMap::from([
                (
                    "enabled".to_string(),
                    ProstValue {
                        kind: Some(Kind::BoolValue(true)),
                    },
                ),
                (
                    "ratio".to_string(),
                    ProstValue {
                        kind: Some(Kind::NumberValue(1.25)),
                    },
                ),
                (
                    "names".to_string(),
                    ProstValue {
                        kind: Some(Kind::ListValue(ListValue {
                            values: vec![
                                ProstValue {
                                    kind: Some(Kind::StringValue("alpha".into())),
                                },
                                ProstValue {
                                    kind: Some(Kind::NullValue(0)),
                                },
                                ProstValue { kind: None },
                            ],
                        })),
                    },
                ),
            ]),
        })),
    };

    assert_eq!(
        prost_value_json(&nested),
        serde_json::json!({
            "enabled": true,
            "ratio": 1.25,
            "names": ["alpha", null, null],
        })
    );
}

#[test]
fn experimental_config_singleton_emits_update() {
    let mut resources = ResourceMap::new();
    resources.insert(
        EXPERIMENTAL_CONFIG_FILE.file_path.into(),
        Resource {
            resource_id: "default".into(),
            name: EXPERIMENTAL_CONFIG_FILE.name.into(),
            file_path: EXPERIMENTAL_CONFIG_FILE.file_path.into(),
            payload: serde_json::json!({
                "content": r#"{ "flag_test": true }"#
            }),
        },
    );
    let mut commands = Vec::new();
    append_experimental_config_update(&mut commands, &resources, &serde_json::json!({}), &None);
    let command = commands
        .iter()
        .find(|command| command.r#type == "experimental_config_update_config")
        .expect("experimental config update command");
    match &command.payload {
        Some(CommandPayload::ExperimentalConfigUpdateConfig(payload)) => {
            assert_eq!(payload.id, "default");
        }
        _ => panic!("unexpected payload variant for experimental config update command"),
    }
}
