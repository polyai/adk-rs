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
        Some(timestamp.clone()),
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
