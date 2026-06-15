use crate::{
    build_push_commands, build_push_commands_with_created_by, projection_to_resource_map,
    try_build_push_commands_with_metadata,
};
use adk_protobuf::command::Payload as CommandPayload;
use adk_types::{Resource, ResourceMap};

fn sample_topic_resource(content: &str) -> Resource {
    Resource {
        resource_id: "local".to_string(),
        name: "sample".to_string(),
        file_path: "topics/sample.yaml".to_string(),
        payload: serde_json::json!({
            "content": format!(
                "name: sample\nenabled: true\nactions: \"\"\ncontent: \"{content}\"\nexample_queries: []\n"
            )
        }),
    }
}

#[test]
fn builds_create_topic_command_when_remote_missing() {
    let mut resources = ResourceMap::new();
    resources.insert(
        "topics/sample.yaml".to_string(),
        sample_topic_resource("hello"),
    );
    let projection = serde_json::json!({});
    let commands = build_push_commands(&resources, &projection);
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].r#type, "create_topic");
    assert!(commands[0].metadata.is_some());
    assert!(matches!(
        commands[0].payload,
        Some(CommandPayload::CreateTopic(_))
    ));
}

#[test]
fn create_topic_parses_typed_local_yaml_and_derives_references() {
    let mut resources = ResourceMap::new();
    resources.insert(
        "topics/support.yaml".to_string(),
        Resource {
            resource_id: "local".to_string(),
            name: "support".to_string(),
            file_path: "topics/support.yaml".to_string(),
            payload: serde_json::json!({
                "content": "name: support\nenabled: true\nactions: \"Call {{fn:start_verification}} and send {{twilio_sms:welcome}}\"\ncontent: \"Use {{vrbl:customer_name}} and {{tn:greeting}}\"\nexample_queries:\n  - help\n"
            }),
        },
    );
    let projection = serde_json::json!({
        "functions": {
            "functions": {
                "entities": {
                    "FUNCTION-start-verification": {
                        "name": "start_verification"
                    }
                }
            }
        },
        "sms": {
            "templates": {
                "entities": {
                    "SMS-welcome": {
                        "name": "welcome",
                        "active": true
                    }
                }
            }
        },
        "variables": {
            "variables": {
                "entities": {
                    "VARIABLE-customer-name": {
                        "name": "customer_name"
                    }
                }
            }
        },
        "translations": {
            "translations": {
                "entities": {
                    "TRANSLATION-greeting": {
                        "translationKey": "greeting"
                    }
                }
            }
        }
    });

    let commands = build_push_commands(&resources, &projection);
    let create = commands
        .iter()
        .find_map(|command| match &command.payload {
            Some(CommandPayload::CreateTopic(create)) => Some(create),
            _ => None,
        })
        .expect("create topic command");

    assert_eq!(
        create.actions,
        "Call {{fn:FUNCTION-start-verification}} and send {{twilio_sms:SMS-welcome}}"
    );
    assert_eq!(
        create.content,
        "Use {{vrbl:VARIABLE-customer-name}} and {{tn:TRANSLATION-greeting}}"
    );
    let references = create.references.as_ref().expect("topic references");
    assert!(
        references
            .global_functions
            .get("FUNCTION-start-verification")
            .copied()
            .unwrap_or(false)
    );
    assert!(references.sms.get("SMS-welcome").copied().unwrap_or(false));
    assert!(
        references
            .variables
            .get("VARIABLE-customer-name")
            .copied()
            .unwrap_or(false)
    );
    assert!(
        references
            .translations
            .get("TRANSLATION-greeting")
            .copied()
            .unwrap_or(false)
    );
}

#[test]
fn create_topic_payload_parsing_does_not_enforce_validation_rules() {
    let mut resources = ResourceMap::new();
    resources.insert(
        "topics/topic_1.yaml".to_string(),
        Resource {
            resource_id: "local".to_string(),
            name: "topic_1".to_string(),
            file_path: "topics/topic_1.yaml".to_string(),
            payload: serde_json::json!({
                "content": "# <<<<<<< ours\nname: Ours\nenabled: true\nactions: \"\"\ncontent: \"hello\"\nexample_queries: []\n# =======\n# >>>>>>> theirs\n"
            }),
        },
    );

    let commands = build_push_commands(&resources, &serde_json::json!({}));
    let create = commands
        .iter()
        .find_map(|command| match &command.payload {
            Some(CommandPayload::CreateTopic(create)) => Some(create),
            _ => None,
        })
        .expect("create topic command");

    assert_eq!(create.name, "Ours");
    assert_eq!(create.content, "hello");
}

#[test]
fn create_topic_uses_local_resource_id_before_synthetic_fallback() {
    let mut resources = ResourceMap::new();
    resources.insert(
        "topics/sample.yaml".to_string(),
        Resource {
            resource_id: "TOPIC-custom-id".to_string(),
            name: "sample".to_string(),
            file_path: "topics/sample.yaml".to_string(),
            payload: serde_json::json!({
                "content": "name: sample\nenabled: true\nactions: \"\"\ncontent: \"hello\"\nexample_queries: []\n"
            }),
        },
    );
    let projection = serde_json::json!({});
    let commands = build_push_commands(&resources, &projection);
    let create_cmd = commands
        .iter()
        .find(|c| c.r#type == "create_topic")
        .expect("create topic command");
    match &create_cmd.payload {
        Some(CommandPayload::CreateTopic(msg)) => assert_eq!(msg.id, "TOPIC-custom-id"),
        _ => panic!("unexpected payload variant for create topic command"),
    }
}

#[test]
fn push_commands_can_use_supplied_projection_and_created_by() {
    let mut resources = ResourceMap::new();
    resources.insert(
        "topics/sample.yaml".to_string(),
        Resource {
            resource_id: "topic-1".to_string(),
            name: "sample".to_string(),
            file_path: "topics/sample.yaml".to_string(),
            payload: serde_json::json!({
                "content": "name: sample\nenabled: true\nactions: \"\"\ncontent: \"local\"\nexample_queries: []\n"
            }),
        },
    );
    let projection = serde_json::json!({
        "knowledgeBase": {
            "topics": {
                "entities": {
                    "topic-1": {
                        "name": "sample",
                        "isActive": true,
                        "actions": "",
                        "content": "remote",
                        "exampleQueries": []
                    }
                }
            }
        }
    });

    let commands =
        build_push_commands_with_created_by(&resources, &projection, Some("reviewer@example.com"));

    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].r#type, "update_topic");
    assert_eq!(
        commands[0].metadata.as_ref().map(|m| m.created_by.as_str()),
        Some("reviewer@example.com")
    );
}

#[test]
fn push_commands_default_to_python_sdk_user_metadata() {
    let mut resources = ResourceMap::new();
    resources.insert(
        "topics/sample.yaml".to_string(),
        sample_topic_resource("hello"),
    );

    let commands = build_push_commands(&resources, &serde_json::json!({}));

    assert_eq!(
        commands[0].metadata.as_ref().map(|m| m.created_by.as_str()),
        Some("sdk-user")
    );
}

#[test]
fn push_commands_can_use_supplied_metadata_timestamp() {
    let mut resources = ResourceMap::new();
    resources.insert(
        "topics/sample.yaml".to_string(),
        sample_topic_resource("hello"),
    );

    let timestamp = prost_types::Timestamp {
        seconds: 1_781_008_496,
        nanos: 123_000_000,
    };
    let commands = try_build_push_commands_with_metadata(
        &resources,
        &serde_json::json!({}),
        Some("reviewer@example.com"),
        Some(timestamp),
    )
    .expect("build commands");

    assert_eq!(commands.len(), 1);
    let metadata = commands[0].metadata.as_ref().expect("metadata");
    assert_eq!(metadata.created_by, "reviewer@example.com");
    assert_eq!(metadata.created_at, Some(timestamp));
}

#[test]
fn builds_delete_topic_command_when_local_removed() {
    let resources = ResourceMap::new();
    let projection = serde_json::json!({
        "knowledgeBase": {
            "topics": {
                "entities": {
                    "topic-1": {
                        "name": "sample",
                        "actions": "",
                        "content": "hello"
                    }
                }
            }
        }
    });
    let commands = build_push_commands(&resources, &projection);
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].r#type, "delete_topic");
    assert!(matches!(
        commands[0].payload,
        Some(CommandPayload::DeleteTopic(_))
    ));
}

#[test]
fn projection_to_resource_map_rejects_duplicate_cleaned_topic_paths() {
    let projection = serde_json::json!({
        "knowledgeBase": {
            "topics": {
                "entities": {
                    "topic-1": {"name": "Billing-Team", "content": "one"},
                    "topic-2": {"name": "Billing Team", "content": "two"}
                }
            }
        }
    });

    let error = projection_to_resource_map(&projection)
        .expect_err("duplicate cleaned topic paths should fail")
        .to_string();
    assert!(error.contains("Duplicate resource file path found"));
    assert!(error.contains("topics/billing_team.yaml"));
    assert!(error.contains("Please rename the resource to avoid conflicts."));
}
