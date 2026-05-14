use super::*;
use serde_json::{Value, json};

#[derive(Clone, Copy)]
enum Operation {
    Create,
    Update,
    Delete,
}

#[derive(Clone, Copy)]
enum ResourceKind {
    Topic,
    GlobalFunction,
    StartFunction,
    EndFunction,
    Variable,
    Entity,
    Handoff,
    SmsTemplate,
    PhraseFilter,
    Flow,
    FlowFunctionStep,
    FlowTransitionFunction,
}

/// One executable row in the in-memory parity matrix.
///
/// Use this for broad local coverage where a resource family, subtype, and
/// lifecycle operation should deterministically emit Python-compatible command
/// types. Keep HTTP/server-specific contracts in recording fixtures.
struct Case {
    name: &'static str,
    kind: ResourceKind,
    operation: Operation,
    expected_commands: &'static [&'static str],
}

impl Case {
    fn run(&self) {
        let resources = resources_for(self.kind, self.operation);
        let projection = projection_for(self.kind, self.operation);
        let commands = build_phase1_commands(&resources, &projection);
        let command_types = commands
            .iter()
            .map(|command| command.r#type.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            command_types, self.expected_commands,
            "{}: unexpected command types",
            self.name
        );
    }
}

#[test]
fn push_command_lifecycle_matrix() {
    // A row is one cheap in-memory parity contract: local resources plus a
    // projection must emit the same command type sequence as Python ADK.
    // Add rows here for breadth; keep server-specific contracts in recordings.
    let cases = [
        Case {
            name: "topic create",
            kind: ResourceKind::Topic,
            operation: Operation::Create,
            expected_commands: &["create_topic"],
        },
        Case {
            name: "topic update",
            kind: ResourceKind::Topic,
            operation: Operation::Update,
            expected_commands: &["update_topic"],
        },
        Case {
            name: "topic delete",
            kind: ResourceKind::Topic,
            operation: Operation::Delete,
            expected_commands: &["delete_topic"],
        },
        Case {
            name: "global function create",
            kind: ResourceKind::GlobalFunction,
            operation: Operation::Create,
            expected_commands: &["create_function"],
        },
        Case {
            name: "global function update",
            kind: ResourceKind::GlobalFunction,
            operation: Operation::Update,
            expected_commands: &["update_function"],
        },
        Case {
            name: "global function delete",
            kind: ResourceKind::GlobalFunction,
            operation: Operation::Delete,
            expected_commands: &["delete_function"],
        },
        Case {
            name: "start function create",
            kind: ResourceKind::StartFunction,
            operation: Operation::Create,
            expected_commands: &["create_start_function"],
        },
        Case {
            name: "start function update",
            kind: ResourceKind::StartFunction,
            operation: Operation::Update,
            expected_commands: &["update_start_function"],
        },
        Case {
            name: "start function delete",
            kind: ResourceKind::StartFunction,
            operation: Operation::Delete,
            expected_commands: &["delete_start_function"],
        },
        Case {
            name: "end function create",
            kind: ResourceKind::EndFunction,
            operation: Operation::Create,
            expected_commands: &["create_end_function"],
        },
        Case {
            name: "end function update",
            kind: ResourceKind::EndFunction,
            operation: Operation::Update,
            expected_commands: &["update_end_function"],
        },
        Case {
            name: "end function delete",
            kind: ResourceKind::EndFunction,
            operation: Operation::Delete,
            expected_commands: &["delete_end_function"],
        },
        Case {
            name: "variable create",
            kind: ResourceKind::Variable,
            operation: Operation::Create,
            expected_commands: &["variable_create"],
        },
        Case {
            name: "variable delete",
            kind: ResourceKind::Variable,
            operation: Operation::Delete,
            expected_commands: &["variable_delete"],
        },
        Case {
            name: "entity create",
            kind: ResourceKind::Entity,
            operation: Operation::Create,
            expected_commands: &["entity_create"],
        },
        Case {
            name: "entity update",
            kind: ResourceKind::Entity,
            operation: Operation::Update,
            expected_commands: &["entity_update"],
        },
        Case {
            name: "entity delete",
            kind: ResourceKind::Entity,
            operation: Operation::Delete,
            expected_commands: &["entity_delete"],
        },
        Case {
            name: "handoff create",
            kind: ResourceKind::Handoff,
            operation: Operation::Create,
            expected_commands: &["handoff_create"],
        },
        Case {
            name: "handoff update",
            kind: ResourceKind::Handoff,
            operation: Operation::Update,
            expected_commands: &["handoff_update"],
        },
        Case {
            name: "handoff delete",
            kind: ResourceKind::Handoff,
            operation: Operation::Delete,
            expected_commands: &["handoff_delete"],
        },
        Case {
            name: "sms template create",
            kind: ResourceKind::SmsTemplate,
            operation: Operation::Create,
            expected_commands: &["sms_create_template"],
        },
        Case {
            name: "sms template update",
            kind: ResourceKind::SmsTemplate,
            operation: Operation::Update,
            expected_commands: &["sms_update_template"],
        },
        Case {
            name: "sms template delete",
            kind: ResourceKind::SmsTemplate,
            operation: Operation::Delete,
            expected_commands: &["sms_delete_template"],
        },
        Case {
            name: "phrase filter create",
            kind: ResourceKind::PhraseFilter,
            operation: Operation::Create,
            expected_commands: &["stop_keywords_create"],
        },
        Case {
            name: "phrase filter update",
            kind: ResourceKind::PhraseFilter,
            operation: Operation::Update,
            expected_commands: &["stop_keywords_update"],
        },
        Case {
            name: "phrase filter delete",
            kind: ResourceKind::PhraseFilter,
            operation: Operation::Delete,
            expected_commands: &["stop_keywords_delete"],
        },
        Case {
            name: "flow create",
            kind: ResourceKind::Flow,
            operation: Operation::Create,
            expected_commands: &["create_flow"],
        },
        Case {
            name: "flow update",
            kind: ResourceKind::Flow,
            operation: Operation::Update,
            expected_commands: &["update_flow"],
        },
        Case {
            name: "flow delete",
            kind: ResourceKind::Flow,
            operation: Operation::Delete,
            expected_commands: &["delete_flow"],
        },
        Case {
            name: "flow function step create",
            kind: ResourceKind::FlowFunctionStep,
            operation: Operation::Create,
            expected_commands: &["create_step"],
        },
        Case {
            name: "flow function step update",
            kind: ResourceKind::FlowFunctionStep,
            operation: Operation::Update,
            expected_commands: &["update_step"],
        },
        Case {
            name: "flow function step delete",
            kind: ResourceKind::FlowFunctionStep,
            operation: Operation::Delete,
            expected_commands: &["delete_step"],
        },
        Case {
            name: "flow transition function create",
            kind: ResourceKind::FlowTransitionFunction,
            operation: Operation::Create,
            expected_commands: &["create_flow_transition_function"],
        },
        Case {
            name: "flow transition function update",
            kind: ResourceKind::FlowTransitionFunction,
            operation: Operation::Update,
            expected_commands: &["update_flow_transition_function"],
        },
        Case {
            name: "flow transition function delete",
            kind: ResourceKind::FlowTransitionFunction,
            operation: Operation::Delete,
            expected_commands: &["delete_flow_transition_function"],
        },
    ];

    for case in cases {
        case.run();
    }
}

#[test]
fn flow_function_step_create_uses_python_empty_latency_control_shape() {
    let commands = build_phase1_commands(
        &resources_for(ResourceKind::FlowFunctionStep, Operation::Create),
        &projection_for(ResourceKind::FlowFunctionStep, Operation::Create),
    );
    let create = commands
        .iter()
        .find(|command| command.r#type == "create_step")
        .expect("create_step command");
    let summary = command_to_json_summary(create);

    assert_eq!(
        summary.pointer("/create_step/function_step/function/latency_control"),
        Some(&json!({}))
    );
}

#[test]
fn projection_materialization_matrix() {
    struct MaterializationCase {
        name: &'static str,
        projection: Value,
        expected_paths: &'static [&'static str],
    }

    let cases = [
        MaterializationCase {
            name: "topic",
            projection: topic_projection("remote content"),
            expected_paths: &["topics/parity_topic.yaml"],
        },
        MaterializationCase {
            name: "global function",
            projection: global_function_projection("return {'remote': True}"),
            expected_paths: &["functions/parity_function.py"],
        },
        MaterializationCase {
            name: "special functions",
            projection: json!({
                "specialFunctions": {
                    "startFunction": remote_special_function("start-1", "start_function", "return None"),
                    "endFunction": remote_special_function("end-1", "end_function", "return None")
                }
            }),
            expected_paths: &["functions/start_function.py", "functions/end_function.py"],
        },
        MaterializationCase {
            name: "flow config and steps",
            projection: flow_projection(),
            expected_paths: &[
                "flows/parity_flow/flow_config.yaml",
                "flows/parity_flow/steps/start.yaml",
                "flows/parity_flow/function_steps/check_account.py",
                "flows/parity_flow/functions/route_account.py",
            ],
        },
    ];

    for case in cases {
        let resources = projection_to_resource_map(&case.projection)
            .unwrap_or_else(|error| panic!("{}: materialization failed: {error}", case.name));
        for path in case.expected_paths {
            assert!(
                resources.contains_key(*path),
                "{}: expected materialized path {path}",
                case.name
            );
        }
    }
}

fn resources_for(kind: ResourceKind, operation: Operation) -> ResourceMap {
    if matches!(operation, Operation::Delete)
        && !matches!(
            kind,
            ResourceKind::FlowFunctionStep | ResourceKind::FlowTransitionFunction
        )
    {
        return ResourceMap::new();
    }

    let mut resources = ResourceMap::new();
    let resource = match kind {
        ResourceKind::Topic => resource(
            "topics/parity_topic.yaml",
            "parity_topic",
            json!({
                "content": "name: parity_topic\nenabled: true\nactions: \"\"\ncontent: \"local content\"\nexample_queries: []\n"
            }),
        ),
        ResourceKind::GlobalFunction => resource(
            "functions/parity_function.py",
            "parity_function",
            json!({
                "content": "@func_description('Local description')\ndef parity_function(conv: Conversation):\n    return {'local': True}\n"
            }),
        ),
        ResourceKind::StartFunction => resource(
            "functions/start_function.py",
            "start_function",
            json!({
                "content": "@func_description('Local start')\ndef start_function(conv: Conversation):\n    return None\n"
            }),
        ),
        ResourceKind::EndFunction => resource(
            "functions/end_function.py",
            "end_function",
            json!({
                "content": "@func_description('Local end')\ndef end_function(conv: Conversation):\n    return None\n"
            }),
        ),
        ResourceKind::Variable => resource(
            "variables/CustomerId",
            "CustomerId",
            json!({ "content": "" }),
        ),
        ResourceKind::Entity => resource(
            "config/entities.yaml",
            "entities",
            json!({
                "content": "entities:\n- name: CustomerId\n  description: Local entity\n  entity_type: free_text\n  config: {}\n"
            }),
        ),
        ResourceKind::Handoff => resource(
            "config/handoffs.yaml",
            "handoffs",
            json!({
                "content": "handoffs:\n- name: Front Desk\n  description: Local handoff\n  is_default: false\n  sip_config:\n    method: refer\n    phone_number: '+200'\n  sip_headers: []\n"
            }),
        ),
        ResourceKind::SmsTemplate => resource(
            "config/sms_templates.yaml",
            "sms_templates",
            json!({
                "content": "sms_templates:\n- name: appointment_reminder\n  text: Local reminder\n  env_phone_numbers:\n    sandbox: ''\n    pre_release: ''\n    live: '+200'\n"
            }),
        ),
        ResourceKind::PhraseFilter => resource(
            "voice/response_control/phrase_filtering.yaml",
            "phrase_filtering",
            json!({
                "content": "phrase_filtering:\n- name: Block Competitor\n  description: Local phrase filter\n  regular_expressions:\n  - '\\\\bcompetitor\\\\b'\n  say_phrase: true\n  language_code: en-US\n"
            }),
        ),
        ResourceKind::Flow => {
            insert_flow_resources(&mut resources, "Local flow", false, false);
            return resources;
        }
        ResourceKind::FlowFunctionStep => {
            insert_flow_resources(
                &mut resources,
                "Remote flow",
                !matches!(operation, Operation::Delete),
                false,
            );
            return resources;
        }
        ResourceKind::FlowTransitionFunction => {
            insert_flow_resources(
                &mut resources,
                "Remote flow",
                false,
                !matches!(operation, Operation::Delete),
            );
            return resources;
        }
    };
    resources.insert(resource.file_path.clone(), resource);
    resources
}

fn projection_for(kind: ResourceKind, operation: Operation) -> Value {
    if matches!(operation, Operation::Create)
        && !matches!(
            kind,
            ResourceKind::FlowFunctionStep | ResourceKind::FlowTransitionFunction
        )
    {
        return json!({});
    }

    match kind {
        ResourceKind::Topic => topic_projection("remote content"),
        ResourceKind::GlobalFunction => global_function_projection("return {'remote': True}"),
        ResourceKind::StartFunction => json!({
            "specialFunctions": {
                "startFunction": remote_special_function("start-1", "start_function", "return 'remote'")
            }
        }),
        ResourceKind::EndFunction => json!({
            "specialFunctions": {
                "endFunction": remote_special_function("end-1", "end_function", "return 'remote'")
            }
        }),
        ResourceKind::Variable => json!({
            "variables": {
                "variables": {
                    "entities": {
                        "variable-1": { "id": "variable-1", "name": "CustomerId" }
                    }
                }
            }
        }),
        ResourceKind::Entity => entity_projection("Remote entity"),
        ResourceKind::Handoff => handoff_projection("Remote handoff", "+100"),
        ResourceKind::SmsTemplate => sms_projection("Remote reminder", "+100"),
        ResourceKind::PhraseFilter => phrase_filter_projection("Remote phrase filter"),
        ResourceKind::Flow => flow_projection_with_description("Remote flow", false, false),
        ResourceKind::FlowFunctionStep => flow_projection_with_description(
            "Remote flow",
            !matches!(operation, Operation::Create),
            false,
        ),
        ResourceKind::FlowTransitionFunction => flow_projection_with_description(
            "Remote flow",
            false,
            !matches!(operation, Operation::Create),
        ),
    }
}

fn resource(path: &str, name: &str, payload: Value) -> Resource {
    Resource {
        resource_id: "local".to_string(),
        name: name.to_string(),
        file_path: path.to_string(),
        payload,
    }
}

fn topic_projection(content: &str) -> Value {
    json!({
        "knowledgeBase": {
            "topics": {
                "entities": {
                    "topic-1": {
                        "id": "topic-1",
                        "name": "parity_topic",
                        "isActive": true,
                        "actions": "",
                        "content": content,
                        "exampleQueries": []
                    }
                }
            }
        }
    })
}

fn global_function_projection(body: &str) -> Value {
    json!({
        "functions": {
            "functions": {
                "entities": {
                    "function-1": {
                        "id": "function-1",
                        "name": "parity_function",
                        "description": "Remote description",
                        "code": format!("def parity_function(conv: Conversation):\n    {body}\n"),
                        "archived": false
                    }
                }
            }
        }
    })
}

fn remote_special_function(id: &str, name: &str, body: &str) -> Value {
    json!({
        "id": id,
        "name": name,
        "description": "Remote description",
        "code": format!("def {name}(conv: Conversation):\n    {body}\n"),
        "archived": false
    })
}

fn entity_projection(description: &str) -> Value {
    json!({
        "entities": {
            "entities": {
                "entities": {
                    "entity-1": {
                        "id": "entity-1",
                        "name": "CustomerId",
                        "description": description,
                        "type": "FreeText",
                        "config": {}
                    }
                }
            }
        }
    })
}

fn handoff_projection(description: &str, phone_number: &str) -> Value {
    json!({
        "handoff": {
            "handoffs": {
                "entities": {
                    "handoff-1": {
                        "id": "handoff-1",
                        "name": "Front Desk",
                        "description": description,
                        "sipConfig": {
                            "config": {
                                "$case": "refer",
                                "value": { "phoneNumber": phone_number }
                            }
                        },
                        "sipHeaders": { "headers": [] },
                        "active": true,
                        "isDefault": false
                    }
                }
            }
        }
    })
}

fn sms_projection(text: &str, live_number: &str) -> Value {
    json!({
        "sms": {
            "templates": {
                "entities": {
                    "sms-1": {
                        "id": "sms-1",
                        "name": "appointment_reminder",
                        "text": text,
                        "envPhoneNumbers": {
                            "sandbox": "",
                            "preRelease": "",
                            "live": live_number
                        },
                        "active": true
                    }
                }
            }
        }
    })
}

fn phrase_filter_projection(description: &str) -> Value {
    json!({
        "stopKeywords": {
            "filters": {
                "entities": {
                    "phrase-1": {
                        "id": "phrase-1",
                        "title": "Block Competitor",
                        "description": description,
                        "regularExpressions": ["\\bcompetitor\\b"],
                        "sayPhrase": true,
                        "languageCode": "en-US"
                    }
                }
            }
        }
    })
}

fn insert_flow_resources(
    resources: &mut ResourceMap,
    description: &str,
    include_function_step: bool,
    include_transition_function: bool,
) {
    let flow_config = resource(
        "flows/parity_flow/flow_config.yaml",
        "Parity Flow",
        json!({
            "content": format!(
                "name: Parity Flow\ndescription: {description}\nstart_step: Start\n"
            )
        }),
    );
    resources.insert(flow_config.file_path.clone(), flow_config);

    let start_step = resource(
        "flows/parity_flow/steps/start.yaml",
        "Start",
        json!({
            "content": "name: Start\ntype: default\nprompt: Collect the account id.\nconditions: []\n"
        }),
    );
    resources.insert(start_step.file_path.clone(), start_step);

    if include_function_step {
        let function_step = resource(
            "flows/parity_flow/function_steps/check_account.py",
            "check_account",
            json!({
                "content": "def check_account(flow: Flow):\n    return None\n"
            }),
        );
        resources.insert(function_step.file_path.clone(), function_step);
    }
    if include_transition_function {
        let transition_function = resource(
            "flows/parity_flow/functions/route_account.py",
            "route_account",
            json!({
                "content": "@func_description('Local transition')\ndef route_account(conv: Conversation, flow: Flow):\n    return None\n"
            }),
        );
        resources.insert(transition_function.file_path.clone(), transition_function);
    }
}

fn flow_projection() -> Value {
    flow_projection_with_description("Flow used by in-memory parity matrix tests.", true, true)
}

fn flow_projection_with_description(
    description: &str,
    include_function_step: bool,
    include_transition_function: bool,
) -> Value {
    let mut steps = serde_json::Map::new();
    steps.insert(
        "step-1".to_string(),
        json!({
            "id": "step-1",
            "name": "Start",
            "type": "default",
            "prompt": "Collect the account id."
        }),
    );
    if include_function_step {
        steps.insert(
            "function-step-1".to_string(),
            json!({
                "id": "function-step-1",
                "name": "check_account",
                "type": "function_step",
                "function": {
                    "code": "def check_account(flow: Flow):\n    return 'remote'\n"
                }
            }),
        );
    }
    let transition_functions = if include_transition_function {
        json!({
            "entities": {
                "transition-1": {
                    "id": "transition-1",
                    "name": "route_account",
                    "description": "Remote transition",
                    "code": "def route_account(conv: Conversation, flow: Flow):\n    return 'remote'\n",
                    "archived": false
                }
            }
        })
    } else {
        json!({ "entities": {} })
    };
    json!({
        "flows": {
            "flows": {
                "entities": {
                    "flow-1": {
                        "id": "flow-1",
                        "name": "Parity Flow",
                        "description": description,
                        "startStepId": "step-1",
                        "steps": {
                            "entities": steps
                        },
                        "transitionFunctions": transition_functions
                    }
                }
            }
        }
    })
}
