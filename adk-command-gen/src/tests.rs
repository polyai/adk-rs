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
        build_phase1_commands_with_actor(&resources, &projection, Some("reviewer@example.com"));

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
        Resource {
            resource_id: "local".to_string(),
            name: "sample".to_string(),
            file_path: "topics/sample.yaml".to_string(),
            payload: serde_json::json!({
                "content": "name: sample\nenabled: true\nactions: \"\"\ncontent: \"hello\"\nexample_queries: []\n"
            }),
        },
    );

    let commands = build_phase1_commands(&resources, &serde_json::json!({}));

    assert_eq!(
        commands[0].metadata.as_ref().map(|m| m.created_by.as_str()),
        Some("sdk-user")
    );
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
fn projection_function_with_distinct_display_name_round_trips_without_commands() {
    let projection = serde_json::json!({
        "functions": {
            "functions": {
                "entities": {
                    "fn-1": {
                        "id": "fn-1",
                        "name": "Lookup Customer",
                        "description": "Looks up a customer.",
                        "code": "def lookup_customer(conv):\n    return 'ok'\n",
                        "archived": false
                    }
                }
            }
        }
    });
    let resources = projection_to_resource_map(&projection).expect("projection resources");
    let commands = build_phase1_commands(&resources, &projection);

    assert!(commands.is_empty());
}

#[test]
fn archived_remote_function_absent_from_disk_is_not_deleted() {
    let projection = serde_json::json!({
        "functions": {
            "functions": {
                "entities": {
                    "fn-1": {
                        "id": "fn-1",
                        "name": "Archived Function",
                        "code": "def archived_function(conv):\n    return 'old'\n",
                        "archived": true
                    }
                }
            }
        }
    });
    let commands = build_phase1_commands(&ResourceMap::new(), &projection);

    assert!(
        commands
            .iter()
            .all(|command| command.r#type != "delete_function")
    );
}

#[test]
fn create_function_infers_description_and_parameters_from_code() {
    let mut resources = ResourceMap::new();
    resources.insert(
        "functions/new_func.py".to_string(),
        Resource {
            resource_id: "functions/new_func.py".to_string(),
            name: "new_func".to_string(),
            file_path: "functions/new_func.py".to_string(),
            payload: serde_json::json!({
                "content": "@func_description('Create greeting.')\n@func_parameter('name', 'Customer name')\n@func_parameter('age', 'Customer age')\ndef new_func(conv: Conversation, name: str, age: int = 0):\n    conv.state.customer_name = name\n    return f'Hi {name}'\n"
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
            assert!(msg.id.starts_with("FUNCTIONS-"));
            assert_ne!(msg.id, "functions/new_func.py");
            assert_eq!(msg.description, "Create greeting.");
            assert_eq!(msg.parameters.len(), 2);
            assert_eq!(msg.parameters[0].name, "name");
            assert_eq!(msg.parameters[0].description, "Customer name");
            assert_eq!(msg.parameters[0].r#type, "string");
            assert_eq!(msg.parameters[1].name, "age");
            assert_eq!(msg.parameters[1].description, "Customer age");
            assert_eq!(msg.parameters[1].r#type, "integer");
            assert!(
                msg.code
                    .starts_with("def new_func(conv: Conversation, name: str, age: int = 0):")
            );
            assert!(
                msg.references
                    .as_ref()
                    .is_some_and(|refs| !refs.variables.is_empty())
            );
        }
        _ => panic!("unexpected payload variant for create function command"),
    }
}

#[test]
fn function_latency_control_decorator_populates_create_and_update_commands() {
    let mut resources = ResourceMap::new();
    resources.insert(
        "functions/slow_lookup.py".to_string(),
        Resource {
            resource_id: "local".to_string(),
            name: "slow_lookup".to_string(),
            file_path: "functions/slow_lookup.py".to_string(),
            payload: serde_json::json!({
                "content": "@func_latency_control(delay_before_responses_start=3, silence_after_each_response=1, delay_responses=[('Please wait', 2)])\ndef slow_lookup(conv: Conversation):\n    return None\n"
            }),
        },
    );

    let create_commands = build_phase1_commands(&resources, &serde_json::json!({}));
    let create = create_commands
        .iter()
        .find(|c| c.r#type == "create_function")
        .expect("create function command");
    match &create.payload {
        Some(CommandPayload::CreateFunction(msg)) => {
            let latency = msg.latency_control.as_ref().expect("latency control");
            assert!(latency.enabled);
            assert_eq!(latency.initial_delay, 3);
            assert_eq!(latency.interval, 1);
            assert_eq!(latency.delay_responses[0].message, "Please wait");
            assert!(!msg.code.contains("func_latency_control"));
        }
        _ => panic!("unexpected payload variant for create function command"),
    }

    let projection = serde_json::json!({
        "functions": {
            "functions": {
                "entities": {
                    "fn-1": {
                        "id": "fn-1",
                        "name": "slow_lookup",
                        "description": "",
                        "code": "def slow_lookup(conv: Conversation):\n    return None\n",
                        "latencyControl": {
                            "enabled": false,
                            "initialDelay": 0,
                            "interval": 0,
                            "delayResponses": {"entities": {}, "ids": []}
                        }
                    }
                }
            }
        }
    });
    let update_commands = build_phase1_commands(&resources, &projection);
    let update = update_commands
        .iter()
        .find(|c| c.r#type == "update_latency_control")
        .expect("update latency control command");
    match &update.payload {
        Some(CommandPayload::UpdateLatencyControl(msg)) => {
            assert_eq!(msg.function_id, "fn-1");
            assert!(msg.enabled);
            assert_eq!(msg.initial_delay, Some(3));
            assert_eq!(msg.interval, Some(1));
        }
        _ => panic!("unexpected payload variant for update latency command"),
    }
}

#[test]
fn transition_function_latency_control_decorator_emits_flow_scoped_update() {
    let mut resources = ResourceMap::new();
    resources.insert(
        "flows/parity_flow/flow_config.yaml".to_string(),
        local_resource(
            "flows/parity_flow/flow_config.yaml",
            "parity_flow",
            "name: parity_flow\ndescription: Test flow\nstart_step: start\n",
        ),
    );
    resources.insert(
        "flows/parity_flow/functions/route_account.py".to_string(),
        local_resource(
            "flows/parity_flow/functions/route_account.py",
            "route_account",
            "@func_latency_control(delay_before_responses_start=2, silence_after_each_response=1, delay_responses=[('Routing', 2)])\ndef route_account(conv: Conversation, flow: Flow):\n    return None\n",
        ),
    );
    let projection = serde_json::json!({
        "flows": {
            "flows": {
                "entities": {
                    "flow-1": {
                        "id": "flow-1",
                        "name": "parity_flow",
                        "description": "Test flow",
                        "startStepId": "start",
                        "steps": {"entities": {}, "ids": []},
                        "transitionFunctions": {
                            "entities": {
                                "tf-1": {
                                    "id": "tf-1",
                                    "name": "route_account",
                                    "description": "",
                                    "code": "def route_account(conv: Conversation, flow: Flow):\n    return None\n",
                                    "latencyControl": {
                                        "enabled": false,
                                        "initialDelay": 0,
                                        "interval": 0,
                                        "delayResponses": {"entities": {}, "ids": []}
                                    }
                                }
                            },
                            "ids": ["tf-1"]
                        }
                    }
                }
            }
        }
    });

    let commands = build_phase1_commands(&resources, &projection);
    let update = commands
        .iter()
        .find(|c| c.r#type == "update_flow_transition_function_latency_control")
        .expect("transition latency control update");
    match &update.payload {
        Some(CommandPayload::UpdateFlowTransitionFunctionLatencyControl(msg)) => {
            assert_eq!(msg.flow_id, "flow-1");
            let latency = msg.latency_control.as_ref().expect("latency control");
            assert_eq!(latency.function_id, "tf-1");
            assert!(latency.enabled);
            assert_eq!(latency.initial_delay, Some(2));
        }
        _ => panic!("unexpected payload variant for transition latency update"),
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
    let start_content = start
        .payload
        .get("content")
        .and_then(serde_json::Value::as_str)
        .expect("start function content");
    let end_content = end
        .payload
        .get("content")
        .and_then(serde_json::Value::as_str)
        .expect("end function content");
    assert!(start_content.contains("@func_description('Runs at call start.')"));
    assert!(end_content.contains("def end_function"));
}

#[test]
fn projection_materializes_global_functions_as_raw_content() {
    let projection = serde_json::json!({
        "functions": {
            "functions": {
                "entities": {
                    "fn-1": {
                        "name": "lookup_customer",
                        "description": "Look up a customer.",
                        "code": "from functions.helpers import normalize\n\n\ndef lookup_customer(conv: Conversation):\n    return normalize({'ok': True})\n"
                    }
                }
            }
        }
    });

    let resources = projection_to_resource_map(&projection).expect("projection resources");
    let function = resources
        .get("functions/lookup_customer.py")
        .expect("function resource");
    let content = function
        .payload
        .get("content")
        .and_then(serde_json::Value::as_str)
        .expect("function content");

    assert!(!content.contains("from _gen import *  # <AUTO GENERATED>"));
    assert_eq!(
        content,
        "from functions.helpers import normalize\n\n\n@func_description('Look up a customer.')\ndef lookup_customer(conv: Conversation):\n    return normalize({'ok': True})\n"
    );
}

#[test]
fn multiline_function_metadata_decorators_match_python_ast_behavior() {
    let content = r#"from _gen import *  # <AUTO GENERATED>

@func_description(
    "Transfers a caller."
)
@func_parameter(
    "handoff_reason",
    "Reason copied from the instruction context.",
)
def handoff(conv: Conversation, handoff_reason: str):
    return {"reason": handoff_reason}
"#;

    assert_eq!(
        push_functions::infer_function_description(content),
        "Transfers a caller."
    );
    let parameters = push_functions::infer_function_parameters(content);
    assert_eq!(parameters.len(), 1);
    assert_eq!(parameters[0].name, "handoff_reason");
    assert_eq!(
        parameters[0].description,
        "Reason copied from the instruction context."
    );
    assert_eq!(parameters[0].r#type, "string");
    assert_eq!(
        push_functions::function_code_from_local_content(content),
        "def handoff(conv: Conversation, handoff_reason: str):\n    return {\"reason\": handoff_reason}\n"
    );
}

#[test]
fn projection_ignores_archived_global_functions() {
    let projection = serde_json::json!({
        "functions": {
            "functions": {
                "entities": {
                    "fn-active": {
                        "name": "lookup_customer",
                        "description": "Look up a customer.",
                        "code": "def lookup_customer(conv):\n    return True\n",
                        "archived": false
                    },
                    "fn-archived": {
                        "name": "lookup_customer",
                        "description": "Archived duplicate.",
                        "code": "def lookup_customer(conv):\n    return False\n",
                        "archived": true
                    }
                }
            }
        }
    });

    let resources = projection_to_resource_map(&projection).expect("projection resources");

    assert!(resources.contains_key("functions/lookup_customer.py"));
    assert_eq!(resources.len(), 1);
    assert_eq!(
        resources
            .get("functions/lookup_customer.py")
            .map(|resource| resource.resource_id.as_str()),
        Some("fn-active")
    );
}

#[test]
fn projection_materializes_safety_filters_as_python_yaml_shape() {
    let projection = serde_json::json!({
        "contentFilterSettings": {
            "disabled": true,
            "type": "azure",
            "azureConfig": {
                "violence": {"isActive": true, "precision": "STRICT"},
                "hate": {"isActive": false, "precision": "MEDIUM"},
                "sexual": {"isActive": false, "precision": "LOOSE"},
                "selfHarm": {"isActive": true, "precision": "STRICT"}
            }
        },
        "channels": {
            "voice": {
                "config": {
                    "safetyFilters": {
                        "disabled": false,
                        "azureConfig": {
                            "violence": {"isActive": true, "precision": "STRICT"},
                            "hate": {"isActive": true, "precision": "MEDIUM"},
                            "sexual": {"isActive": false, "precision": "LOOSE"},
                            "selfHarm": {"isActive": false, "precision": "MEDIUM"}
                        }
                    }
                }
            }
        }
    });

    let resources = projection_to_resource_map(&projection).expect("projection resources");
    let general = resources
        .get("agent_settings/safety_filters.yaml")
        .and_then(|resource| resource.payload.get("content"))
        .and_then(serde_json::Value::as_str)
        .expect("general safety filter YAML");
    let voice = resources
        .get("voice/safety_filters.yaml")
        .and_then(|resource| resource.payload.get("content"))
        .and_then(serde_json::Value::as_str)
        .expect("voice safety filter YAML");

    assert!(!general.contains("azureConfig"));
    assert!(!general.contains("disabled:"));
    assert!(general.contains("categories:"));
    assert!(general.contains("self_harm:"));
    assert!(general.contains("level: strict"));
    assert!(voice.contains("enabled: true"));
}

#[test]
fn projection_materializes_channel_configuration_as_python_yaml_shape() {
    let projection = serde_json::json!({
        "channels": {
            "voice": {
                "config": {
                    "greeting": {
                        "welcomeMessage": "Hello",
                        "languageCode": "en-US"
                    },
                    "stylePrompt": {"prompt": "Warm and concise"}
                },
                "disclaimer": {
                    "message": "Recorded line",
                    "isEnabled": true,
                    "languageCode": "en-US"
                }
            },
            "webChat": {
                "status": true,
                "config": {
                    "greeting": {
                        "welcomeMessage": "Hi",
                        "languageCode": "en-GB"
                    },
                    "stylePrompt": {"prompt": "Helpful"}
                }
            }
        }
    });

    let resources = projection_to_resource_map(&projection).expect("projection resources");
    let voice = resources
        .get("voice/configuration.yaml")
        .and_then(|resource| resource.payload.get("content"))
        .and_then(serde_json::Value::as_str)
        .expect("voice configuration YAML");
    let chat = resources
        .get("chat/configuration.yaml")
        .and_then(|resource| resource.payload.get("content"))
        .and_then(serde_json::Value::as_str)
        .expect("chat configuration YAML");

    assert!(voice.contains("welcome_message: Hello"));
    assert!(voice.contains("language_code: en-US"));
    assert!(voice.contains("disclaimer_messages:"));
    assert!(voice.contains("enabled: true"));
    assert!(!voice.contains("welcomeMessage"));
    assert!(!voice.contains("- message:"));
    assert!(chat.contains("welcome_message: Hi"));
    assert!(chat.contains("style_prompt:"));
}

#[test]
fn projection_materializes_asr_settings_as_python_yaml_shape() {
    let projection = serde_json::json!({
        "channels": {
            "voice": {
                "asrSettings": {
                    "bargeIn": false,
                    "latencyConfig": {
                        "interactionStyle": "precise"
                    },
                    "updatedAt": "2026-01-21T14:35:16.078Z",
                    "updatedBy": "miles.nash@poly-ai.com"
                }
            }
        }
    });

    let resources = projection_to_resource_map(&projection).expect("projection resources");
    let content = resources
        .get("voice/speech_recognition/asr_settings.yaml")
        .and_then(|resource| resource.payload.get("content"))
        .and_then(serde_json::Value::as_str)
        .expect("ASR settings YAML");

    assert!(content.contains("barge_in: false"));
    assert!(content.contains("interaction_style: precise"));
    assert!(!content.contains("bargeIn"));
    assert!(!content.contains("latencyConfig"));
    assert!(!content.contains("updatedAt"));
    assert!(!content.contains("updatedBy"));
}

#[test]
fn projection_materializes_agent_settings_as_python_yaml_shape() {
    let projection = serde_json::json!({
        "agentSettings": {
            "personality": {
                "adjectives": {"values": {"Calm": true}},
                "custom": "Be helpful",
                "createdAt": "ignored"
            },
            "role": {
                "value": "other",
                "additionalInfo": "Receptionist",
                "custom": "Custom role",
                "updatedAt": "ignored"
            }
        }
    });

    let resources = projection_to_resource_map(&projection).expect("projection resources");
    let personality = resources
        .get("agent_settings/personality.yaml")
        .and_then(|resource| resource.payload.get("content"))
        .and_then(serde_json::Value::as_str)
        .expect("personality YAML");
    let role = resources
        .get("agent_settings/role.yaml")
        .and_then(|resource| resource.payload.get("content"))
        .and_then(serde_json::Value::as_str)
        .expect("role YAML");

    assert!(personality.contains("adjectives:"));
    assert!(personality.contains("custom: Be helpful"));
    assert!(!personality.contains("createdAt"));
    assert!(role.contains("additional_info: Receptionist"));
    assert!(!role.contains("additionalInfo"));
    assert!(!role.contains("updatedAt"));
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

#[test]
fn projection_to_resource_map_rejects_duplicate_cleaned_flow_step_paths() {
    let projection = serde_json::json!({
        "flows": {
            "flows": {
                "entities": {
                    "flow-1": {
                        "id": "flow-1",
                        "name": "Support Flow",
                        "startStepId": "step-1",
                        "steps": {
                            "entities": {
                                "step-1": {
                                    "name": "Collect-Info",
                                    "type": "advanced_step",
                                    "prompt": "one"
                                },
                                "step-2": {
                                    "name": "Collect Info",
                                    "type": "advanced_step",
                                    "prompt": "two"
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    let error = projection_to_resource_map(&projection)
        .expect_err("duplicate cleaned flow-step paths should fail")
        .to_string();
    assert!(error.contains("Duplicate resource file path found"));
    assert!(error.contains("flows/support_flow/steps/collect_info.yaml"));
}

#[test]
fn projection_materializes_flow_step_yaml_with_python_key_casing() {
    let projection = serde_json::json!({
        "flows": {
            "flows": {
                "entities": {
                    "flow-1": {
                        "id": "flow-1",
                        "name": "Support Flow",
                        "steps": {
                            "entities": {
                                "step-1": {
                                    "name": "Collect Rating",
                                    "type": "advanced_step",
                                    "prompt": "Rate the call",
                                    "position": {
                                        "x": 100.0,
                                        "y": 200.0
                                    },
                                    "asrBiasing": {
                                        "customKeywords": ["billing"]
                                    },
                                    "dtmfConfig": {
                                        "isEnabled": true,
                                        "interDigitTimeout": 3,
                                        "isPii": false
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    let resources = projection_to_resource_map(&projection).expect("projection resources");
    let content = resources
        .get("flows/support_flow/steps/collect_rating.yaml")
        .and_then(|resource| resource.payload.get("content"))
        .and_then(serde_json::Value::as_str)
        .expect("flow step YAML");

    assert!(content.contains("asr_biasing:"));
    assert!(content.contains("custom_keywords:"));
    assert!(content.contains("dtmf_config:"));
    assert!(content.contains("inter_digit_timeout: 3"));
    assert!(content.contains("is_pii: false"));
    assert!(!content.contains("customKeywords"));
    assert!(!content.contains("dtmfConfig"));
    assert!(!content.contains("position:"));
}

#[test]
fn projection_materializes_named_prompt_references_like_python() {
    let projection = serde_json::json!({
        "functions": {
            "functions": {
                "entities": {
                    "FUNCTION-start_verification": {
                        "id": "FUNCTION-start_verification",
                        "name": "start_verification",
                        "code": "def start_verification(conv):\n    return {}\n",
                        "archived": false
                    }
                }
            }
        },
        "variables": {
            "variables": {
                "entities": {
                    "VARIABLE-call_direction_prompt": {
                        "id": "VARIABLE-call_direction_prompt",
                        "name": "call_direction_prompt"
                    }
                }
            }
        },
        "variantManagement": {
            "variants": {
                "entities": {
                    "VAR-default": {
                        "name": "default",
                        "isDefault": true
                    }
                }
            },
            "attributes": {
                "entities": {
                    "ATTR-site_name": {
                        "id": "ATTR-site_name",
                        "name": "site_name",
                        "archived": false
                    }
                }
            },
            "variantAttributeValues": {
                "entities": {}
            }
        },
        "knowledgeBase": {
            "topics": {
                "entities": {
                    "TOPIC-1": {
                        "id": "TOPIC-1",
                        "name": "Billing",
                        "actions": "Call {{fn:FUNCTION-start_verification}} using {{attr:ATTR-site_name}}",
                        "content": "Use {{vrbl:VARIABLE-call_direction_prompt}} in replies",
                        "exampleQueries": [],
                        "isActive": true
                    }
                }
            }
        },
        "agentSettings": {
            "rules": {
                "behaviour": "Rules {{fn:FUNCTION-start_verification}} {{attr:ATTR-site_name}} {{vrbl:VARIABLE-call_direction_prompt}}"
            }
        },
        "flows": {
            "flows": {
                "entities": {
                    "FLOW-address": {
                        "id": "FLOW-address",
                        "name": "Address Flow",
                        "startStepId": "STEP-determine_language",
                        "steps": {
                            "entities": {
                                "STEP-determine_language": {
                                    "id": "STEP-determine_language",
                                    "name": "Determine Language",
                                    "type": "advanced_step",
                                    "prompt": "Step {{fn:FUNCTION-start_verification}} {{ft:FUNCTION-determine_language}} {{attr:ATTR-site_name}} {{vrbl:VARIABLE-call_direction_prompt}}"
                                }
                            }
                        },
                        "transitionFunctions": {
                            "entities": {
                                "FUNCTION-determine_language": {
                                    "id": "FUNCTION-determine_language",
                                    "name": "determine_language",
                                    "code": "def determine_language(conv):\n    return {}\n",
                                    "archived": false
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    let resources = projection_to_resource_map(&projection).expect("projection resources");

    let rules = resources
        .get("agent_settings/rules.txt")
        .and_then(|resource| resource.payload.get("content"))
        .and_then(serde_json::Value::as_str)
        .expect("rules");
    assert!(rules.contains("{{fn:start_verification}}"));
    assert!(rules.contains("{{attr:site_name}}"));
    assert!(rules.contains("{{vrbl:call_direction_prompt}}"));
    assert!(!rules.contains("FUNCTION-start_verification"));
    assert!(!rules.contains("ATTR-site_name"));
    assert!(!rules.contains("VARIABLE-call_direction_prompt"));

    let topic = resources
        .get("topics/billing.yaml")
        .and_then(|resource| resource.payload.get("content"))
        .and_then(serde_json::Value::as_str)
        .expect("topic");
    assert!(topic.contains("{{fn:start_verification}}"));
    assert!(topic.contains("{{attr:site_name}}"));
    assert!(topic.contains("{{vrbl:call_direction_prompt}}"));
    assert!(!topic.contains("FUNCTION-start_verification"));

    let step = resources
        .get("flows/address_flow/steps/determine_language.yaml")
        .and_then(|resource| resource.payload.get("content"))
        .and_then(serde_json::Value::as_str)
        .expect("step");
    assert!(step.contains("{{fn:start_verification}}"));
    assert!(step.contains("{{ft:determine_language}}"));
    assert!(step.contains("{{attr:site_name}}"));
    assert!(step.contains("{{vrbl:call_direction_prompt}}"));
    assert!(!step.contains("FUNCTION-determine_language"));
}

#[test]
fn reference_named_materialization_round_trips_without_push_commands() {
    let projection = serde_json::json!({
        "functions": {
            "functions": {
                "entities": {
                    "FUNCTION-start_verification": {
                        "id": "FUNCTION-start_verification",
                        "name": "start_verification",
                        "code": "def start_verification(conv):\n    return {}\n",
                        "archived": false
                    }
                }
            }
        },
        "variantManagement": {
            "variants": {
                "entities": {
                    "VAR-default": {
                        "name": "default",
                        "isDefault": true
                    }
                }
            },
            "attributes": {
                "entities": {
                    "ATTR-site_name": {
                        "id": "ATTR-site_name",
                        "name": "site_name",
                        "archived": false
                    }
                }
            },
            "variantAttributeValues": {
                "entities": {}
            }
        },
        "knowledgeBase": {
            "topics": {
                "entities": {
                    "TOPIC-1": {
                        "id": "TOPIC-1",
                        "name": "Billing",
                        "actions": "Call {{fn:FUNCTION-start_verification}} using {{attr:ATTR-site_name}}",
                        "content": "Use {{attr:ATTR-site_name}} in replies",
                        "exampleQueries": [],
                        "isActive": true
                    }
                }
            }
        },
        "agentSettings": {
            "rules": {
                "behaviour": "Rules {{fn:FUNCTION-start_verification}} {{attr:ATTR-site_name}}"
            }
        },
        "flows": {
            "flows": {
                "entities": {
                    "FLOW-address": {
                        "id": "FLOW-address",
                        "name": "Address Flow",
                        "startStepId": "STEP-determine_language",
                        "steps": {
                            "entities": {
                                "STEP-determine_language": {
                                    "id": "STEP-determine_language",
                                    "name": "Determine Language",
                                    "type": "advanced_step",
                                    "prompt": "Step {{fn:FUNCTION-start_verification}} {{ft:FUNCTION-determine_language}} {{attr:ATTR-site_name}}"
                                }
                            }
                        },
                        "transitionFunctions": {
                            "entities": {
                                "FUNCTION-determine_language": {
                                    "id": "FUNCTION-determine_language",
                                    "name": "determine_language",
                                    "code": "def determine_language(conv):\n    return {}\n",
                                    "archived": false
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    let resources = projection_to_resource_map(&projection).expect("projection resources");
    let commands = build_phase1_commands(&resources, &projection);
    assert!(
        commands.is_empty(),
        "expected no commands, got types: {:?}",
        commands
            .iter()
            .map(|command| command.r#type.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn projection_materializes_flow_function_imports_with_human_readable_paths() {
    let projection = serde_json::json!({
        "functions": {
            "functions": {
                "entities": {
                    "FUNCTION-add_hcpc_item_back_after_education": {
                        "id": "FUNCTION-add_hcpc_item_back_after_education",
                        "name": "add_hcpc_item_back_after_education",
                        "code": "from functions.flow_435908a2.answers_item_question import answers_item_question\n\ndef add_hcpc_item_back_after_education(conv):\n    return answers_item_question(conv)\n",
                        "archived": false
                    }
                }
            }
        },
        "flows": {
            "flows": {
                "entities": {
                    "FLOW-435908a2": {
                        "id": "FLOW-435908a2",
                        "name": "Rapid Reorder",
                        "steps": {
                            "entities": {}
                        },
                        "transitionFunctions": {
                            "entities": {
                                "FUNCTION-answers_item_question": {
                                    "id": "FUNCTION-answers_item_question",
                                    "name": "answers_item_question",
                                    "code": "def answers_item_question(conv, flow):\n    return {}\n",
                                    "archived": false
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    let resources = projection_to_resource_map(&projection).expect("projection resources");
    let content = resources
        .get("functions/add_hcpc_item_back_after_education.py")
        .and_then(|resource| resource.payload.get("content"))
        .and_then(serde_json::Value::as_str)
        .expect("global function code");
    assert!(content.contains(
        "from flows.rapid_reorder.functions.answers_item_question import answers_item_question"
    ));
    assert!(!content.contains("from functions.flow_435908a2.answers_item_question"));
}

#[test]
fn flow_function_import_pretty_paths_round_trip_without_push_commands() {
    let projection = serde_json::json!({
        "functions": {
            "functions": {
                "entities": {
                    "FUNCTION-add_hcpc_item_back_after_education": {
                        "id": "FUNCTION-add_hcpc_item_back_after_education",
                        "name": "add_hcpc_item_back_after_education",
                        "code": "from functions.flow_435908a2.answers_item_question import answers_item_question\n\ndef add_hcpc_item_back_after_education(conv):\n    return answers_item_question(conv)\n",
                        "archived": false
                    }
                }
            }
        },
        "flows": {
            "flows": {
                "entities": {
                    "FLOW-435908a2": {
                        "id": "FLOW-435908a2",
                        "name": "Rapid Reorder",
                        "steps": {
                            "entities": {}
                        },
                        "transitionFunctions": {
                            "entities": {
                                "FUNCTION-answers_item_question": {
                                    "id": "FUNCTION-answers_item_question",
                                    "name": "answers_item_question",
                                    "code": "def answers_item_question(conv, flow):\n    return {}\n",
                                    "archived": false
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    let resources = projection_to_resource_map(&projection).expect("projection resources");
    let commands = build_phase1_commands(&resources, &projection);
    assert!(
        commands.is_empty(),
        "expected no commands, got types: {:?}",
        commands
            .iter()
            .map(|command| command.r#type.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn projection_materializes_flow_config_start_step_as_step_name() {
    let projection = serde_json::json!({
        "flows": {
            "flows": {
                "entities": {
                    "flow-1": {
                        "id": "flow-1",
                        "name": "Support Flow",
                        "startStepId": "step-1",
                        "steps": {
                            "entities": {
                                "Support Flow_step-1": {
                                    "name": "Collect Rating",
                                    "type": "advanced_step",
                                    "prompt": "Rate the call"
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    let resources = projection_to_resource_map(&projection).expect("projection resources");
    let content = resources
        .get("flows/support_flow/flow_config.yaml")
        .and_then(|resource| resource.payload.get("content"))
        .and_then(serde_json::Value::as_str)
        .expect("flow config YAML");

    assert!(content.contains("start_step: Collect Rating"));
    assert!(!content.contains("start_step: step-1"));
}

#[test]
fn projection_materializes_flow_config_start_step_as_id_when_step_is_missing() {
    let projection = serde_json::json!({
        "flows": {
            "flows": {
                "entities": {
                    "flow-1": {
                        "id": "flow-1",
                        "name": "Support Flow",
                        "startStepId": "STEP-start",
                        "steps": {
                            "entities": {
                                "STEP-other": {
                                    "name": "Collect Rating",
                                    "type": "advanced_step",
                                    "prompt": "Rate the call"
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    let resources = projection_to_resource_map(&projection).expect("projection resources");
    let content = resources
        .get("flows/support_flow/flow_config.yaml")
        .and_then(|resource| resource.payload.get("content"))
        .and_then(serde_json::Value::as_str)
        .expect("flow config YAML");

    assert!(content.contains("start_step: STEP-start"));
    assert!(!content.contains("start_step: Collect Rating"));
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
fn push_builder_appends_variable_commands() {
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
fn push_builder_follows_global_delete_create_update_order() {
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
fn projection_to_resource_map_includes_single_file_resource_files() {
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
fn projection_materializes_broad_resources_without_python_omitted_metadata() {
    let projection = serde_json::json!({
        "pronunciations": {"pronunciations": {"entities": {
            "pron-1": {
                "name": "Display name",
                "regex": "ADK",
                "replacement": "Agent Development Kit",
                "caseSensitive": false,
                "languageCode": "",
                "description": "",
                "position": 4
            }
        }}},
        "transcriptCorrections": {"transcriptCorrections": {"entities": {
            "correction-1": {
                "name": "ADK correction",
                "description": "",
                "regularExpressions": [{
                    "id": "regex-1",
                    "regularExpression": "agent development kid",
                    "replacement": "agent development kit",
                    "replacementType": "full"
                }]
            }
        }}},
        "stopKeywords": {"filters": {"entities": {
            "stop-1": {
                "title": "Hang Up",
                "description": "",
                "regularExpressions": ["bye"],
                "sayPhrase": false,
                "languageCode": ""
            }
        }}},
        "variantManagement": {
            "variants": {"entities": {
                "variant-default": {"name": "default", "isDefault": true},
                "variant-other": {"name": "other", "isDefault": false}
            }},
            "attributes": {"entities": {}},
            "variantAttributeValues": {"entities": {}}
        }
    });

    let map = projection_to_resource_map(&projection).expect("map");
    let pronunciations = map
        .get("voice/response_control/pronunciations.yaml")
        .and_then(|r| r.payload.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("");
    assert!(pronunciations.contains("regex: ADK"));
    assert!(!pronunciations.contains("name:"));
    assert!(!pronunciations.contains("position:"));
    assert!(!pronunciations.contains("description: ''"));
    assert!(!pronunciations.contains("language_code: ''"));

    let transcript_corrections = map
        .get("voice/speech_recognition/transcript_corrections.yaml")
        .and_then(|r| r.payload.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("");
    assert!(transcript_corrections.contains("regular_expression: agent development kid"));
    assert!(!transcript_corrections.contains("id: regex-1"));

    let phrase_filtering = map
        .get("voice/response_control/phrase_filtering.yaml")
        .and_then(|r| r.payload.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("");
    assert!(phrase_filtering.contains("name: Hang Up"));
    assert!(!phrase_filtering.contains("language_code: ''"));
    assert!(!phrase_filtering.contains("description: ''"));

    let variants = map
        .get("config/variant_attributes.yaml")
        .and_then(|r| r.payload.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("");
    assert!(variants.contains("is_default: true"));
    assert!(!variants.contains("is_default: false"));
}

#[test]
fn single_file_resource_files_emit_real_create_commands() {
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
            "missing single-file create command: {expected}"
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
fn single_file_settings_files_emit_real_update_commands() {
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
            "missing single-file update command: {expected}"
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

fn local_resource(path: &str, name: &str, content: &str) -> Resource {
    Resource {
        resource_id: "local".to_string(),
        name: name.to_string(),
        file_path: path.to_string(),
        payload: serde_json::json!({ "content": content }),
    }
}
