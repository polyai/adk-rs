use super::payload_json_summary;
use crate::test_support::local_resource;
use crate::{build_push_commands, projection_to_resource_map};
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::flows::{
    AdvancedStepCondition, ConditionDetails, ExitFlowCondition, FlowUpdateTransitionFunction,
    FunctionStepCondition, NoCodeStepCondition, StepPosition, TransitionFunctionReferences,
    TransitionFunctionUpdateTransitionFunction, UpdateNoCodeCondition, update_no_code_condition,
};
use adk_protobuf::functions::{ErrorsUpdate, ParametersUpdate};
use adk_types::ResourceMap;
use serde_json::Value;
use std::collections::HashMap;

#[test]
fn update_transition_function_sends_empty_parameters_to_delete_remote_parameters() {
    let mut resources = ResourceMap::new();
    resources.insert(
        "flows/support/flow_config.yaml".to_string(),
        local_resource(
            "flows/support/flow_config.yaml",
            "support",
            "name: support\ndescription: Support flow\nstart_step: collect\n",
        ),
    );
    resources.insert(
        "flows/support/functions/route.py".to_string(),
        local_resource(
            "flows/support/functions/route.py",
            "route",
            "def route(conv: Conversation, flow: Flow):\n    return 'new'\n",
        ),
    );
    let projection = serde_json::json!({
        "flows": {
            "flows": {
                "entities": {
                    "flow-1": {
                        "id": "flow-1",
                        "name": "support",
                        "description": "Support flow",
                        "startStepId": "collect",
                        "steps": {"entities": {}, "ids": []},
                        "transitionFunctions": {
                            "entities": {
                                "tf-1": {
                                    "id": "tf-1",
                                    "name": "route",
                                    "description": "",
                                    "code": "def route(conv: Conversation, flow: Flow, customer_id: str):\n    return customer_id\n",
                                    "parameters": [
                                        {"id": "param-1", "name": "customer_id", "description": "Customer id", "type": "string"}
                                    ]
                                }
                            },
                            "ids": ["tf-1"]
                        }
                    }
                }
            }
        }
    });

    let commands = build_push_commands(&resources, &projection);
    let update = commands
        .iter()
        .find(|c| c.r#type == "update_flow_transition_function")
        .expect("update transition function command");
    match &update.payload {
        Some(CommandPayload::UpdateFlowTransitionFunction(msg)) => {
            let transition = msg
                .transition_function
                .as_ref()
                .expect("transition function update");
            assert!(
                transition
                    .parameters
                    .as_ref()
                    .is_some_and(|p| p.parameters.is_empty())
            );
        }
        _ => panic!("unexpected payload variant for transition function update"),
    }
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
    let commands = build_push_commands(&resources, &projection);
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
fn projection_to_resource_map_includes_flow_transition_function_decorators() {
    let projection = serde_json::json!({
        "flows": {
            "flows": {
                "entities": {
                    "flow-1": {
                        "id": "flow-1",
                        "name": "verify_flow",
                        "startStepId": "step-1",
                        "steps": {
                            "entities": {
                                "step-1": {
                                    "name": "start",
                                    "type": "advanced_step",
                                    "prompt": "Verify the caller."
                                }
                            }
                        },
                        "transitionFunctions": {
                            "entities": {
                                "tf-1": {
                                    "id": "tf-1",
                                    "name": "route_call",
                                    "description": "Route to the correct department",
                                    "code": "def route_call(conv: Conversation, dept: str):\n    return dept\n",
                                    "parameters": [
                                        {"id": "p1", "name": "dept", "description": "Must be one of \"billing\", \"support\"", "type": "string"}
                                    ],
                                    "latencyControl": {
                                        "enabled": true,
                                        "initialDelay": 2,
                                        "interval": 1,
                                        "delayResponses": {
                                            "entities": {
                                                "dr-1": {"message": "Routing", "duration": 2}
                                            },
                                            "ids": ["dr-1"]
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    });
    let map = projection_to_resource_map(&projection).expect("map");
    let content = map
        .get("flows/verify_flow/functions/route_call.py")
        .and_then(|r| r.payload.get("content"))
        .and_then(Value::as_str)
        .expect("transition function file content");
    assert!(
        content.contains("@func_description("),
        "missing @func_description: {content}"
    );
    assert!(
        content.contains("@func_parameter('dept',"),
        "missing @func_parameter: {content}"
    );
    assert!(
        content.contains("@func_latency_control("),
        "missing @func_latency_control: {content}"
    );
}

#[test]
fn update_transition_function_summary_includes_optional_sections() {
    let mut flow_steps = HashMap::new();
    flow_steps.insert("step-1".to_string(), true);
    let mut variables = HashMap::new();
    variables.insert("var-1".to_string(), false);
    let payload = CommandPayload::UpdateFlowTransitionFunction(FlowUpdateTransitionFunction {
        flow_id: "flow-1".into(),
        transition_function: Some(TransitionFunctionUpdateTransitionFunction {
            id: "fn-1".into(),
            name: Some("route".into()),
            description: Some("choose route".into()),
            parameters: Some(ParametersUpdate { parameters: vec![] }),
            code: Some("def route(conv):\n    return {}".into()),
            errors: Some(ErrorsUpdate { errors: vec![] }),
            references: Some(TransitionFunctionReferences {
                flow_steps,
                variables,
            }),
        }),
    });

    assert_eq!(
        payload_json_summary(&payload),
        Some((
            "update_flow_transition_function",
            serde_json::json!({
                "flow_id": "flow-1",
                "transition_function": {
                    "id": "fn-1",
                    "name": "route",
                    "description": "choose route",
                    "parameters": {},
                    "code": "def route(conv):\n    return {}",
                    "errors": {},
                    "references": {
                        "flow_steps": {"step-1": true},
                        "variables": {"var-1": false},
                    },
                },
            })
        ))
    );
}

#[test]
fn update_no_code_condition_summary_covers_config_variants() {
    let variants = [
        (
            Some(update_no_code_condition::Config::ExitFlowCondition(
                ExitFlowCondition {
                    details: Some(ConditionDetails {
                        label: "done".into(),
                        description: Some("finish".into()),
                        required_entities: vec!["ent-1".into()],
                        position: Some(StepPosition { x: 1.0, y: 2.0 }),
                        ingress_position: "left".into(),
                    }),
                    exit_flow_position: Some(StepPosition { x: 3.0, y: 4.0 }),
                },
            )),
            "exit_flow_condition",
            serde_json::json!({
                "details": {
                    "label": "done",
                    "description": "finish",
                    "required_entities": ["ent-1"],
                    "position": {"x": 1.0, "y": 2.0},
                    "ingress_position": "left",
                },
                "exit_flow_position": {"x": 3.0, "y": 4.0},
            }),
        ),
        (
            Some(update_no_code_condition::Config::StepCondition(
                AdvancedStepCondition {
                    details: None,
                    child_step_id: "step-child".into(),
                },
            )),
            "step_condition",
            serde_json::json!({}),
        ),
        (
            Some(update_no_code_condition::Config::NoCodeStepCondition(
                NoCodeStepCondition {
                    details: None,
                    child_step_id: "nocode-child".into(),
                },
            )),
            "no_code_step_condition",
            serde_json::json!({}),
        ),
        (
            Some(update_no_code_condition::Config::FunctionStepCondition(
                FunctionStepCondition {
                    details: None,
                    child_step_id: "function-child".into(),
                },
            )),
            "function_step_condition",
            serde_json::json!({}),
        ),
    ];

    for (config, expected_key, expected_value) in variants {
        let payload = CommandPayload::UpdateNoCodeCondition(UpdateNoCodeCondition {
            flow_id: "flow-1".into(),
            step_id: "step-1".into(),
            condition_id: "cond-1".into(),
            config,
        });
        let (_, summary) = payload_json_summary(&payload).expect("flow condition summary");
        assert_eq!(summary["flow_id"], "flow-1");
        assert_eq!(summary["step_id"], "step-1");
        assert_eq!(summary["condition_id"], "cond-1");
        assert_eq!(summary[expected_key], expected_value);
    }
}
