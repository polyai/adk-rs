use crate::{build_push_commands, projection_to_resource_map};
use serde_json::Value;

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
