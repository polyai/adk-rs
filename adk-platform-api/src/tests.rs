use super::*;

#[test]
fn builds_create_topic_command_when_remote_missing() {
    let mut resources = ResourceMap::new();
    resources.insert(
        "topics/sample.yaml".to_string(),
        Resource {
            resource_id: "local".to_string(),
            name: "sample".to_string(),
            file_path: "topics/sample.yaml".to_string(),
            payload: serde_json::json!({
                "content": "name: sample\nenabled: true\nactions: \"\"\ncontent: \"hello\"\nexample_queries: []\n"
            }),
        },
    );
    let projection = serde_json::json!({});
    let commands = build_phase1_commands(&resources, &projection);
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
    let commands = build_phase1_commands(&resources, &projection);
    let create_cmd = commands
        .iter()
        .find(|c| c.r#type == "create_topic")
        .expect("create topic command");
    match &create_cmd.payload {
        Some(CommandPayload::CreateTopic(msg)) => assert_eq!(msg.id, "TOPIC-custom-id"),
        other => panic!("unexpected payload: {other:?}"),
    }
}

#[test]
fn push_commands_can_use_supplied_projection_and_actor() {
    let client = HttpPlatformClient {
        client: reqwest::blocking::Client::new(),
        base_url: "http://localhost".to_string(),
        api_key: "test-key".to_string(),
        account_id: "test-account".to_string(),
        project_id: "test-project".to_string(),
        branch_id: "main".to_string(),
    };
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

    let (commands, last_known_sequence) = client
        .build_push_commands_with_options(
            &resources,
            Some(&projection),
            Some("reviewer@example.com"),
        )
        .expect("build commands");

    assert_eq!(last_known_sequence, 0);
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].r#type, "update_topic");
    assert_eq!(
        commands[0].metadata.as_ref().map(|m| m.created_by.as_str()),
        Some("reviewer@example.com")
    );
}

#[test]
fn push_no_changes_uses_python_failure_contract() {
    let client = HttpPlatformClient {
        client: reqwest::blocking::Client::new(),
        base_url: "http://localhost".to_string(),
        api_key: "test-key".to_string(),
        account_id: "test-account".to_string(),
        project_id: "test-project".to_string(),
        branch_id: "main".to_string(),
    };
    let resources = ResourceMap::new();
    let projection = serde_json::json!({});

    let result = client
        .push_resources_with_options(&resources, Some(&projection), None)
        .expect("push result");

    assert!(!result.success);
    assert_eq!(result.message, "No changes detected");
    assert!(result.commands.is_empty());
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
    let commands = build_phase1_commands(&resources, &projection);
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].r#type, "delete_topic");
    assert!(matches!(
        commands[0].payload,
        Some(CommandPayload::DeleteTopic(_))
    ));
}

#[test]
fn update_function_uses_remote_metadata_when_available() {
    let mut resources = ResourceMap::new();
    resources.insert(
        "functions/test.py".to_string(),
        Resource {
            resource_id: "local".to_string(),
            name: "test".to_string(),
            file_path: "functions/test.py".to_string(),
            payload: serde_json::json!({
                "content": "def test(conv):\n    return 'ok'\n"
            }),
        },
    );
    let projection = serde_json::json!({
        "functions": {
            "functions": {
                "entities": {
                    "fn-1": {
                        "name": "test",
                        "description": "Remote description",
                        "parameters": [{"id": "p1", "name": "customer", "description": "Customer id", "type": "string"}],
                        "errors": [{"lineno": 2, "message": "bad", "text": "raise"}],
                        "archived": true
                    }
                }
            }
        }
    });
    let commands = build_phase1_commands(&resources, &projection);
    let update = commands
        .iter()
        .find(|c| c.r#type == "update_function")
        .expect("update function command");
    match &update.payload {
        Some(CommandPayload::UpdateFunction(msg)) => {
            assert_eq!(msg.description.as_deref(), Some("Remote description"));
            assert!(
                msg.parameters
                    .as_ref()
                    .is_some_and(|p| !p.parameters.is_empty())
            );
            assert!(msg.errors.as_ref().is_some_and(|e| !e.errors.is_empty()));
            assert_eq!(msg.archived, Some(true));
        }
        other => panic!("unexpected payload: {other:?}"),
    }
}

#[test]
fn create_function_infers_description_and_parameters_from_code() {
    let mut resources = ResourceMap::new();
    resources.insert(
        "functions/new_func.py".to_string(),
        Resource {
            resource_id: "local".to_string(),
            name: "new_func".to_string(),
            file_path: "functions/new_func.py".to_string(),
            payload: serde_json::json!({
                "content": "def new_func(name, age=0):\n    \"\"\"Create greeting.\"\"\"\n    return f'Hi {name}'\n"
            }),
        },
    );
    let commands = build_phase1_commands(&resources, &serde_json::json!({}));
    let create = commands
        .iter()
        .find(|c| c.r#type == "create_function")
        .expect("create function command");
    match &create.payload {
        Some(CommandPayload::CreateFunction(msg)) => {
            assert_eq!(msg.description, "Create greeting.");
            assert_eq!(msg.parameters.len(), 2);
        }
        other => panic!("unexpected payload: {other:?}"),
    }
}

#[test]
fn phase1_plus_extended_appends_variable_commands() {
    let mut resources = ResourceMap::new();
    resources.insert(
        "topics/sample.yaml".to_string(),
        Resource {
            resource_id: "local".to_string(),
            name: "sample".to_string(),
            file_path: "topics/sample.yaml".to_string(),
            payload: serde_json::json!({
                "content": "name: sample\nenabled: true\nactions: \"\"\ncontent: \"hello\"\nexample_queries: []\n"
            }),
        },
    );
    resources.insert(
        "variables/MyVar".to_string(),
        Resource {
            resource_id: "local".to_string(),
            name: "MyVar".to_string(),
            file_path: "variables/MyVar".to_string(),
            payload: serde_json::json!({ "content": "" }),
        },
    );
    let projection = serde_json::json!({});
    let commands = build_phase1_commands(&resources, &projection);
    let types: Vec<&str> = commands.iter().map(|c| c.r#type.as_str()).collect();
    assert!(types.contains(&"create_topic"));
    assert!(types.contains(&"variable_create"));
}

#[test]
fn phase1_and_extended_follow_global_delete_create_update_order() {
    let mut resources = ResourceMap::new();
    resources.insert(
        "topics/new.yaml".to_string(),
        Resource {
            resource_id: "local".to_string(),
            name: "new".to_string(),
            file_path: "topics/new.yaml".to_string(),
            payload: serde_json::json!({
                "content": "name: new\nenabled: true\nactions: \"\"\ncontent: \"hello\"\nexample_queries: []\n"
            }),
        },
    );
    resources.insert(
        "topics/create_only.yaml".to_string(),
        Resource {
            resource_id: "local".to_string(),
            name: "create_only".to_string(),
            file_path: "topics/create_only.yaml".to_string(),
            payload: serde_json::json!({
                "content": "name: create_only\nenabled: true\nactions: \"\"\ncontent: \"hello\"\nexample_queries: []\n"
            }),
        },
    );
    resources.insert(
        "variables/NewVar".to_string(),
        Resource {
            resource_id: "local".to_string(),
            name: "NewVar".to_string(),
            file_path: "variables/NewVar".to_string(),
            payload: serde_json::json!({"content": ""}),
        },
    );
    resources.insert(
        "variables/FreshVar".to_string(),
        Resource {
            resource_id: "local".to_string(),
            name: "FreshVar".to_string(),
            file_path: "variables/FreshVar".to_string(),
            payload: serde_json::json!({"content": "{\"name\":\"FreshVar\"}"}),
        },
    );
    let projection = serde_json::json!({
        "knowledgeBase": {"topics": {"entities": {"topic-old": {"name": "old"}}}},
        "variables": {"variables": {"entities": {"vrbl-old": {"name": "OldVar"}}}}
    });
    let commands = build_phase1_commands(&resources, &projection);
    let types: Vec<&str> = commands.iter().map(|c| c.r#type.as_str()).collect();
    let delete_topic_idx = types
        .iter()
        .position(|t| *t == "delete_topic")
        .expect("delete_topic");
    let variable_delete_idx = types
        .iter()
        .position(|t| *t == "variable_delete")
        .expect("variable_delete");
    let create_topic_idx = types
        .iter()
        .position(|t| *t == "create_topic")
        .expect("create_topic");
    let variable_create_idx = types
        .iter()
        .position(|t| *t == "variable_create")
        .expect("variable_create");
    assert!(delete_topic_idx < create_topic_idx);
    assert!(variable_delete_idx < variable_create_idx);
    assert!(delete_topic_idx < variable_create_idx);
}

#[test]
fn queue_prioritizes_variable_commands_across_all_phases() {
    let mut resources = ResourceMap::new();
    resources.insert(
        "topics/new.yaml".to_string(),
        Resource {
            resource_id: "local".to_string(),
            name: "new".to_string(),
            file_path: "topics/new.yaml".to_string(),
            payload: serde_json::json!({
                "content": "name: new\nenabled: true\nactions: \"\"\ncontent: \"hello\"\nexample_queries: []\n"
            }),
        },
    );
    resources.insert(
        "variables/NewVar".to_string(),
        Resource {
            resource_id: "local".to_string(),
            name: "NewVar".to_string(),
            file_path: "variables/NewVar".to_string(),
            payload: serde_json::json!({"content": "{\"name\":\"NewVar\"}"}),
        },
    );
    let projection = serde_json::json!({
        "knowledgeBase": {"topics": {"entities": {"topic-old": {"name": "old"}, "topic-new": {"name": "new"}}}},
        "variables": {"variables": {"entities": {"vrbl-old": {"name": "OldVar"}, "vrbl-keep": {"name": "NewVar"}}}}
    });
    let commands = build_phase1_commands(&resources, &projection);
    let types: Vec<&str> = commands.iter().map(|c| c.r#type.as_str()).collect();
    let variable_delete_idx = types
        .iter()
        .position(|t| *t == "variable_delete")
        .expect("variable_delete");
    let topic_delete_idx = types
        .iter()
        .position(|t| *t == "delete_topic")
        .expect("delete_topic");
    let variable_update_idx = types
        .iter()
        .position(|t| *t == "variable_update")
        .expect("variable_update");
    let topic_update_idx = types
        .iter()
        .position(|t| *t == "update_topic")
        .expect("update_topic");
    assert!(variable_delete_idx < topic_delete_idx);
    assert!(variable_update_idx < topic_update_idx);
}

#[test]
fn projection_to_resource_map_includes_extended_resource_files() {
    let projection = serde_json::json!({
        "variables": {"variables": {"entities": {"vrbl-1": {"name": "MyVar"}}}},
        "entities": {"entities": {"entities": {"ent-1": {"name": "Age", "description": "age", "type": "numeric", "numberConfig": {"min": 1, "max": 120}}}}},
        "handoff": {"handoffs": {"entities": {"ho-1": {"name": "Sales", "description": "to sales", "active": true, "isDefault": true, "sipConfig": {"invite": {"phoneNumber": "+1555", "outboundEndpoint": "trunk", "outboundEncryption": "tls"}}, "sipHeaders": {"headers": [{"key": "X-Test", "value": "1"}]}}}}},
        "sms": {"templates": {"entities": {"twilio_sms-1": {"name": "Welcome", "text": "hi", "active": true, "envPhoneNumbers": {"sandbox": "+1", "preRelease": "+2", "live": "+3"}}}}},
        "stopKeywords": {"filters": {"entities": {"sk-1": {"title": "HangUp", "description": "end", "regularExpressions": ["^bye$"], "sayPhrase": false, "languageCode": "en-US"}}}},
        "experimentalConfig": {"experimentalConfigs": {"entities": {"default": {"features": {"foo": true}}}}}
    });
    let map = projection_to_resource_map(&projection).expect("map");
    assert!(map.contains_key("variables/MyVar"));
    assert!(map.contains_key("config/entities.yaml"));
    assert!(map.contains_key("config/handoffs.yaml"));
    assert!(map.contains_key("config/sms_templates.yaml"));
    assert!(map.contains_key("voice/response_control/phrase_filtering.yaml"));
    assert!(map.contains_key("agent_settings/experimental_config.json"));
    let variable_content = map
        .get("variables/MyVar")
        .and_then(|r| r.payload.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("");
    assert!(variable_content.contains("\"name\": \"MyVar\""));
    let entities_content = map
        .get("config/entities.yaml")
        .and_then(|r| r.payload.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("");
    assert!(entities_content.contains("min: 1"));
    assert!(entities_content.contains("max: 120"));
    let handoff_content = map
        .get("config/handoffs.yaml")
        .and_then(|r| r.payload.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("");
    assert!(handoff_content.contains("method: invite"));
    assert!(handoff_content.contains("phone_number: '+1555'"));
    assert!(handoff_content.contains("key: X-Test"));
}

#[test]
fn extract_response_commands_reads_common_response_shapes() {
    let direct = serde_json::json!({
        "commands": [{"type": "create_topic"}]
    });
    assert_eq!(extract_response_commands(&direct).len(), 1);

    let nested_batch = serde_json::json!({
        "commandBatch": {"commands": [{"type": "delete_topic"}]}
    });
    assert_eq!(extract_response_commands(&nested_batch).len(), 1);

    let nested_result = serde_json::json!({
        "result": {"commands": [{"type": "update_topic"}]}
    });
    assert_eq!(extract_response_commands(&nested_result).len(), 1);
}
