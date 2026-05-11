use super::*;
use adk_protobuf::channels::ChannelType;

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
        _ => panic!("unexpected payload variant for create topic command"),
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
        _ => panic!("unexpected payload variant for update function command"),
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
        _ => panic!("unexpected payload variant for create function command"),
    }
}

#[test]
fn projection_materializes_start_and_end_functions_from_special_functions() {
    let projection = serde_json::json!({
        "specialFunctions": {
            "startFunction": {
                "id": "start-1",
                "name": "start_function",
                "description": "Runs at call start.",
                "code": "def start_function(conv):\n    return None\n",
                "archived": false
            },
            "endFunction": {
                "id": "end-1",
                "name": "end_function",
                "description": "Runs at call end.",
                "code": "def end_function(conv):\n    return None\n",
                "archived": false
            }
        }
    });

    let resources = projection_to_resource_map(&projection).expect("projection resources");
    let start = resources
        .get("functions/start_function.py")
        .expect("start function resource");
    let end = resources
        .get("functions/end_function.py")
        .expect("end function resource");

    assert_eq!(start.resource_id, "start-1");
    assert_eq!(end.resource_id, "end-1");
    assert!(
        start
            .payload
            .get("content")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|content| content.contains("@func_description('Runs at call start.')"))
    );
    assert!(
        end.payload
            .get("content")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|content| content.contains("def end_function"))
    );
}

#[test]
fn special_function_paths_emit_start_and_end_commands() {
    let mut resources = ResourceMap::new();
    resources.insert(
        "functions/start_function.py".to_string(),
        Resource {
            resource_id: "start-1".to_string(),
            name: "start_function".to_string(),
            file_path: "functions/start_function.py".to_string(),
            payload: serde_json::json!({
                "content": "@func_description('New start')\ndef start_function(conv):\n    conv.state.ready = True\n"
            }),
        },
    );
    resources.insert(
        "functions/end_function.py".to_string(),
        Resource {
            resource_id: "local".to_string(),
            name: "end_function".to_string(),
            file_path: "functions/end_function.py".to_string(),
            payload: serde_json::json!({
                "content": "def end_function(conv, reason: str):\n    return reason\n"
            }),
        },
    );
    let projection = serde_json::json!({
        "specialFunctions": {
            "startFunction": {
                "id": "start-1",
                "name": "start_function",
                "description": "Old start",
                "code": "def start_function(conv):\n    return None\n",
                "archived": false
            }
        }
    });

    let commands = build_phase1_commands(&resources, &projection);
    let types = commands
        .iter()
        .map(|command| command.r#type.as_str())
        .collect::<Vec<_>>();
    assert_eq!(types, vec!["create_end_function", "update_start_function"]);

    let update = commands
        .iter()
        .find(|command| command.r#type == "update_start_function")
        .expect("start update command");
    match &update.payload {
        Some(CommandPayload::UpdateStartFunction(update)) => {
            assert_eq!(update.id, "start-1");
            assert_eq!(update.description.as_deref(), Some("New start"));
            assert!(
                update
                    .code
                    .as_deref()
                    .is_some_and(|code| code.contains("conv.state.ready"))
            );
        }
        _ => panic!("unexpected start function update payload"),
    }

    let create = commands
        .iter()
        .find(|command| command.r#type == "create_end_function")
        .expect("end create command");
    match &create.payload {
        Some(CommandPayload::CreateEndFunction(create)) => {
            assert_eq!(create.name, "end_function");
            assert!(create.parameters.is_empty());
        }
        _ => panic!("unexpected end function create payload"),
    }
}

#[test]
fn deleting_local_special_function_files_emits_special_delete_commands() {
    let projection = serde_json::json!({
        "specialFunctions": {
            "startFunction": {
                "id": "start-1",
                "name": "start_function",
                "description": "",
                "code": "def start_function(conv):\n    return None\n",
                "archived": false
            },
            "endFunction": {
                "id": "end-1",
                "name": "end_function",
                "description": "",
                "code": "def end_function(conv):\n    return None\n",
                "archived": false
            }
        },
        "functions": {
            "functions": {
                "entities": {
                    "global-1": {
                        "name": "regular",
                        "code": "def regular(conv):\n    return None\n"
                    }
                }
            }
        }
    });

    let commands = build_phase1_commands(&ResourceMap::new(), &projection);
    let types = commands
        .iter()
        .map(|command| command.r#type.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        types,
        vec![
            "delete_start_function",
            "delete_end_function",
            "delete_function"
        ]
    );
    assert!(matches!(
        commands[0].payload,
        Some(CommandPayload::DeleteStartFunction(_))
    ));
    assert!(matches!(
        commands[1].payload,
        Some(CommandPayload::DeleteEndFunction(_))
    ));
    assert!(matches!(
        commands[2].payload,
        Some(CommandPayload::DeleteFunction(_))
    ));
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
        "experimentalConfig": {"experimentalConfigs": {"entities": {"default": {"features": {"foo": true}}}}},
        "channels": {
            "webChat": {
                "status": 1,
                "config": {
                    "greeting": {"welcomeMessage": "Hello in chat", "languageCode": "en-US"},
                    "stylePrompt": {"prompt": "Keep chat concise."},
                    "safetyFilters": {
                        "type": "azure",
                        "disabled": false,
                        "azureConfig": {"violence": {"isActive": true, "precision": "MEDIUM"}}
                    }
                }
            }
        }
    });
    let map = projection_to_resource_map(&projection).expect("map");
    assert!(map.contains_key("config/entities.yaml"));
    assert!(map.contains_key("config/handoffs.yaml"));
    assert!(map.contains_key("config/sms_templates.yaml"));
    assert!(map.contains_key("voice/response_control/phrase_filtering.yaml"));
    assert!(map.contains_key("agent_settings/experimental_config.json"));
    assert!(map.contains_key("chat/configuration.yaml"));
    assert!(map.contains_key("chat/safety_filters.yaml"));
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

    let chat_content = map
        .get("chat/configuration.yaml")
        .and_then(|r| r.payload.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("");
    assert!(chat_content.contains("Hello in chat"));
    assert!(chat_content.contains("Keep chat concise."));
}

#[test]
fn broad_resource_files_emit_real_create_commands() {
    let mut resources = ResourceMap::new();
    resources.insert(
        "config/variant_attributes.yaml".to_string(),
        local_resource(
            "config/variant_attributes.yaml",
            "variant_attributes",
            r#"
variants:
  - name: default
  - name: treatment
attributes:
  - name: adk-recording-cohort
    values:
      default: control
      treatment: treatment
"#,
        ),
    );
    resources.insert(
        "config/api_integrations.yaml".to_string(),
        local_resource(
            "config/api_integrations.yaml",
            "api_integrations",
            r#"
api_integrations:
  - name: adk_recording_api
    description: Recording-only API integration.
    environments:
      sandbox:
        base_url: https://example.invalid/sandbox
        auth_type: none
    operations:
      - name: get_recording_status
        method: GET
        resource: /status
"#,
        ),
    );
    resources.insert(
        "voice/speech_recognition/keyphrase_boosting.yaml".to_string(),
        local_resource(
            "voice/speech_recognition/keyphrase_boosting.yaml",
            "keyphrase_boosting",
            "keyphrases:\n  - keyphrase: ADK parity\n    level: boosted\n",
        ),
    );
    resources.insert(
        "voice/speech_recognition/transcript_corrections.yaml".to_string(),
        local_resource(
            "voice/speech_recognition/transcript_corrections.yaml",
            "transcript_corrections",
            r#"
corrections:
  - name: ADK spelling
    description: Correct ADK spelling.
    regular_expressions:
      - regular_expression: agent development kid
        replacement: agent development kit
        replacement_type: full
"#,
        ),
    );
    resources.insert(
        "voice/response_control/pronunciations.yaml".to_string(),
        local_resource(
            "voice/response_control/pronunciations.yaml",
            "pronunciations",
            r#"
pronunciations:
  - regex: \bADK\b
    replacement: Agent Development Kit
    case_sensitive: true
    language_code: en-US
"#,
        ),
    );

    let commands = build_phase1_commands(&resources, &serde_json::json!({}));
    let types = commands
        .iter()
        .map(|command| command.r#type.as_str())
        .collect::<Vec<_>>();
    for expected in [
        "variant_create_variant",
        "variant_create_attribute",
        "create_api_integration",
        "create_api_integration_operation",
        "create_keyphrase_boosting",
        "create_transcript_corrections",
        "pronunciations_create_pronunciation",
    ] {
        assert!(
            types.contains(&expected),
            "missing broad create command: {expected}"
        );
    }

    let attribute = commands
        .iter()
        .find(|command| command.r#type == "variant_create_attribute")
        .expect("variant_create_attribute command");
    match &attribute.payload {
        Some(CommandPayload::VariantCreateAttribute(payload)) => {
            let values = payload
                .variant_values
                .as_ref()
                .expect("variant values")
                .values
                .values()
                .cloned()
                .collect::<Vec<_>>();
            assert!(values.contains(&"control".to_string()));
            assert!(values.contains(&"treatment".to_string()));
        }
        _ => panic!("unexpected payload variant for variant_create_attribute command"),
    }

    let api = commands
        .iter()
        .find(|command| command.r#type == "create_api_integration")
        .expect("create_api_integration command");
    match &api.payload {
        Some(CommandPayload::CreateApiIntegration(payload)) => {
            assert_eq!(payload.name, "adk_recording_api");
            assert_eq!(
                payload
                    .environments
                    .as_ref()
                    .and_then(|envs| envs.sandbox.as_ref())
                    .map(|env| env.base_url.as_str()),
                Some("https://example.invalid/sandbox")
            );
        }
        _ => panic!("unexpected payload variant for create_api_integration command"),
    }
}

#[test]
fn broad_settings_files_emit_real_update_commands() {
    let mut resources = ResourceMap::new();
    resources.insert(
        "agent_settings/personality.yaml".to_string(),
        local_resource(
            "agent_settings/personality.yaml",
            "personality",
            "adjectives:\n  Curious: true\ncustom: Recording parity custom personality.\n",
        ),
    );
    resources.insert(
        "agent_settings/role.yaml".to_string(),
        local_resource(
            "agent_settings/role.yaml",
            "role",
            "value: CustomerServiceRepresentative\nadditional_info: Recording parity role detail.\ncustom: ''\n",
        ),
    );
    resources.insert(
        "agent_settings/safety_filters.yaml".to_string(),
        local_resource(
            "agent_settings/safety_filters.yaml",
            "safety_filters",
            "enabled: true\ncategories:\n  violence:\n    enabled: true\n    level: medium\n",
        ),
    );
    resources.insert(
        "voice/configuration.yaml".to_string(),
        local_resource(
            "voice/configuration.yaml",
            "voice_configuration",
            r#"
greeting:
  welcome_message: Hello from tests.
  language_code: en-US
style_prompt:
  prompt: Keep it compact.
disclaimer_messages:
  enabled: true
  message: This call may be recorded.
  language_code: en-US
"#,
        ),
    );
    resources.insert(
        "voice/speech_recognition/asr_settings.yaml".to_string(),
        local_resource(
            "voice/speech_recognition/asr_settings.yaml",
            "asr_settings",
            "barge_in: true\ninteraction_style: balanced\n",
        ),
    );
    resources.insert(
        "voice/safety_filters.yaml".to_string(),
        local_resource(
            "voice/safety_filters.yaml",
            "voice_safety_filters",
            "enabled: true\ncategories:\n  violence:\n    enabled: true\n    level: medium\n",
        ),
    );
    resources.insert(
        "chat/configuration.yaml".to_string(),
        local_resource(
            "chat/configuration.yaml",
            "chat_configuration",
            r#"
greeting:
  welcome_message: Hello from chat.
  language_code: en-US
style_prompt:
  prompt: Keep webchat compact.
"#,
        ),
    );
    resources.insert(
        "chat/safety_filters.yaml".to_string(),
        local_resource(
            "chat/safety_filters.yaml",
            "chat_safety_filters",
            "enabled: true\ncategories:\n  hate:\n    enabled: true\n    level: medium\n",
        ),
    );

    let commands = build_phase1_commands(&resources, &serde_json::json!({}));
    let types = commands
        .iter()
        .map(|command| command.r#type.as_str())
        .collect::<Vec<_>>();
    for expected in [
        "update_personality",
        "update_role",
        "update_content_filter_settings",
        "channel_update_greeting",
        "channel_update_style_prompt",
        "channel_update_safety_filters",
        "voice_channel_update_disclaimer",
        "voice_channel_update_asr_settings",
    ] {
        assert!(
            types.contains(&expected),
            "missing broad update command: {expected}"
        );
    }

    let asr = commands
        .iter()
        .find(|command| command.r#type == "voice_channel_update_asr_settings")
        .expect("voice_channel_update_asr_settings command");
    match &asr.payload {
        Some(CommandPayload::VoiceChannelUpdateAsrSettings(payload)) => {
            let settings = payload.asr_settings.as_ref().expect("asr settings");
            assert_eq!(settings.barge_in, Some(true));
            assert_eq!(
                settings
                    .latency_config
                    .as_ref()
                    .map(|config| config.interaction_style.as_str()),
                Some("balanced")
            );
        }
        _ => panic!("unexpected payload variant for voice_channel_update_asr_settings command"),
    }

    let webchat_greeting = commands
        .iter()
        .find(|command| {
            command.r#type == "channel_update_greeting"
                && matches!(
                    command.payload.as_ref(),
                    Some(CommandPayload::ChannelUpdateGreeting(payload))
                        if payload.channel_type == ChannelType::WebChat as i32
                )
        })
        .expect("webchat greeting update");
    match &webchat_greeting.payload {
        Some(CommandPayload::ChannelUpdateGreeting(payload)) => {
            assert_eq!(payload.channel_type, ChannelType::WebChat as i32);
            assert_eq!(
                payload
                    .greeting
                    .as_ref()
                    .and_then(|greeting| greeting.welcome_message.as_deref()),
                Some("Hello from chat.")
            );
        }
        _ => panic!("unexpected payload variant for webchat greeting command"),
    }

    assert!(commands.iter().any(|command| {
        command.r#type == "channel_update_safety_filters"
            && matches!(
                command.payload.as_ref(),
                Some(CommandPayload::ChannelUpdateSafetyFilters(payload))
                    if payload.channel_type == ChannelType::Voice as i32
            )
    }));
    assert!(commands.iter().any(|command| {
        command.r#type == "channel_update_safety_filters"
            && matches!(
                command.payload.as_ref(),
                Some(CommandPayload::ChannelUpdateSafetyFilters(payload))
                    if payload.channel_type == ChannelType::WebChat as i32
            )
    }));
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

fn local_resource(path: &str, name: &str, content: &str) -> Resource {
    Resource {
        resource_id: "local".to_string(),
        name: name.to_string(),
        file_path: path.to_string(),
        payload: serde_json::json!({ "content": content }),
    }
}
