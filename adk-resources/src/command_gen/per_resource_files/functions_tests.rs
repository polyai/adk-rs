use super::*;
use crate::test_support::local_resource;
use crate::{build_phase1_commands, projection_to_resource_map};
use adk_protobuf::command::Payload as CommandPayload;
use adk_types::{Resource, ResourceMap};

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
fn create_function_infers_parameters_from_def_when_display_name_differs() {
    let mut resources = ResourceMap::new();
    resources.insert(
        "functions/lookup_customer.py".to_string(),
        Resource {
            resource_id: "functions/lookup_customer.py".to_string(),
            name: "Lookup Customer".to_string(),
            file_path: "functions/lookup_customer.py".to_string(),
            payload: serde_json::json!({
                "content": "@func_parameter('customer_id', 'Customer id')\ndef lookup_customer (conv: Conversation, customer_id: str):\n    return customer_id\n"
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
            assert_eq!(msg.name, "Lookup Customer");
            assert_eq!(msg.parameters.len(), 1);
            assert_eq!(msg.parameters[0].name, "customer_id");
            assert_eq!(msg.parameters[0].description, "Customer id");
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
                        "parameters": [
                            {"id": "param-1", "name": "customer_id", "description": "Customer id", "type": "string"},
                            {"id": "param-2", "name": "age", "description": "Customer age", "type": "integer"}
                        ],
                        "code": "from functions.helpers import normalize\n\n\ndef lookup_customer(conv: Conversation, customer_id: str, age: int = 0):\n    return normalize({'ok': True})\n"
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
        "from functions.helpers import normalize\n\n\n@func_description('Look up a customer.')\n@func_parameter('customer_id', 'Customer id')\n@func_parameter('age', 'Customer age')\ndef lookup_customer(conv: Conversation, customer_id: str, age: int = 0):\n    return normalize({'ok': True})\n"
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

    assert_eq!(infer_function_description(content), "Transfers a caller.");
    let parameters = infer_function_parameters(content, "handoff");
    assert_eq!(parameters.len(), 1);
    assert_eq!(parameters[0].name, "handoff_reason");
    assert_eq!(
        parameters[0].description,
        "Reason copied from the instruction context."
    );
    assert_eq!(parameters[0].r#type, "string");
    assert_eq!(
        function_code_from_local_content(content),
        "def handoff(conv: Conversation, handoff_reason: str):\n    return {\"reason\": handoff_reason}\n"
    );
}

#[test]
fn inferred_function_parameter_ids_include_function_name() {
    let first = infer_function_parameters(
        "def lookup(conv: Conversation, value: str):\n    return value\n",
        "lookup",
    );
    let second = infer_function_parameters(
        "def update(conv: Conversation, value: str):\n    return value\n",
        "update",
    );

    assert_eq!(first[0].name, "value");
    assert_eq!(second[0].name, "value");
    assert_ne!(first[0].id, second[0].id);
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
