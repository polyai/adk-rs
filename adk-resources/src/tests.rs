use super::*;
use adk_types::{Resource, ResourceMap};
use serde_json::Value;

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
fn projection_materialization_preserves_python_yaml_key_order() {
    let projection = serde_json::json!({
        "knowledgeBase": {
            "topics": {
                "entities": {
                    "topic-1": {
                        "id": "topic-1",
                        "name": "Billing General",
                        "isActive": true,
                        "actions": "Transfer the caller.",
                        "content": "General billing enquiries.",
                        "exampleQueries": [
                            {"query": "Question about my bill"}
                        ]
                    }
                }
            }
        },
        "entities": {
            "entities": {
                "entities": {
                    "entity-1": {
                        "name": "Age",
                        "description": "Customer age",
                        "type": "numeric",
                        "numberConfig": {"min": 1, "max": 120}
                    }
                }
            }
        },
        "functions": {
            "functions": {
                "entities": {
                    "fn-1": {
                        "id": "fn-1",
                        "name": "handoff",
                        "code": "def handoff(conv):\n    return None\n",
                        "archived": false
                    }
                }
            }
        },
        "handoff": {
            "handoffs": {
                "entities": {
                    "handoff-1": {
                        "name": "Support Queue",
                        "description": "Route to support",
                        "isDefault": true,
                        "active": true,
                        "sipConfig": {
                            "config": {
                                "$case": "invite",
                                "value": {
                                    "phoneNumber": "+441234",
                                    "outboundEndpoint": "sip.example.test",
                                    "outboundEncryption": "TLS/SRTP"
                                }
                            }
                        },
                        "sipHeaders": {
                            "headers": [
                                {"key": "X-Team", "value": "Support"}
                            ]
                        }
                    }
                }
            }
        },
        "sms": {
            "templates": {
                "entities": {
                    "sms-1": {
                        "name": "Reminder",
                        "text": "Your appointment is tomorrow.",
                        "active": true,
                        "envPhoneNumbers": {
                            "sandbox": "+100",
                            "preRelease": "+200",
                            "live": "+300"
                        }
                    }
                }
            }
        },
        "stopKeywords": {
            "filters": {
                "entities": {
                    "phrase-1": {
                        "title": "Escalate",
                        "description": "Escalation phrases",
                        "regularExpressions": ["agent", "human"],
                        "sayPhrase": true,
                        "languageCode": "en-GB",
                        "references": {
                            "globalFunctions": {
                                "fn-1": true
                            }
                        }
                    }
                }
            }
        },
        "flows": {
            "flows": {
                "entities": {
                    "flow-1": {
                        "id": "flow-1",
                        "name": "Support Flow",
                        "description": "Collect details",
                        "startStepId": "step-1",
                        "steps": {
                            "entities": {
                                "step-1": {
                                    "name": "Collect Rating",
                                    "type": "advanced_step",
                                    "prompt": "Rate the call",
                                    "asrBiasing": {},
                                    "dtmfConfig": {}
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    let resources = projection_to_resource_map(&projection).expect("projection resources");
    let content = |path: &str| {
        resources
            .get(path)
            .and_then(|resource| resource.payload.get("content"))
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("missing resource content for {path}"))
    };

    assert_eq!(
        content("topics/billing_general.yaml"),
        "name: Billing General\nenabled: true\nactions: Transfer the caller.\ncontent: General billing enquiries.\nexample_queries:\n- Question about my bill\n"
    );
    assert_eq!(
        content("flows/support_flow/flow_config.yaml"),
        "name: Support Flow\ndescription: Collect details\nstart_step: Collect Rating\n"
    );
    assert_eq!(
        content("flows/support_flow/steps/collect_rating.yaml"),
        "step_type: advanced_step\nname: Collect Rating\nasr_biasing:\n  is_enabled: false\n  alphanumeric: false\n  name_spelling: false\n  numeric: false\n  party_size: false\n  precise_date: false\n  relative_date: false\n  single_number: false\n  time: false\n  yes_no: false\n  address: false\n  custom_keywords: []\ndtmf_config:\n  is_enabled: false\n  inter_digit_timeout: 0\n  max_digits: 0\n  end_key: ''\n  collect_while_agent_speaking: false\n  is_pii: false\nprompt: Rate the call\n"
    );
    assert_eq!(
        content("config/entities.yaml"),
        "entities:\n- name: Age\n  description: Customer age\n  entity_type: numeric\n  config:\n    min: 1\n    max: 120\n"
    );
    assert_eq!(
        content("config/handoffs.yaml"),
        "handoffs:\n- name: Support Queue\n  description: Route to support\n  is_default: true\n  sip_config:\n    method: invite\n    phone_number: '+441234'\n    outbound_endpoint: sip.example.test\n    outbound_encryption: TLS/SRTP\n  sip_headers:\n  - key: X-Team\n    value: Support\n"
    );
    assert_eq!(
        content("config/sms_templates.yaml"),
        "sms_templates:\n- name: Reminder\n  text: Your appointment is tomorrow.\n  env_phone_numbers:\n    sandbox: '+100'\n    pre_release: '+200'\n    live: '+300'\n"
    );
    assert_eq!(
        content("voice/response_control/phrase_filtering.yaml"),
        "phrase_filtering:\n- name: Escalate\n  description: Escalation phrases\n  regular_expressions:\n  - agent\n  - human\n  say_phrase: true\n  language_code: en-GB\n  function: handoff\n"
    );
}

#[test]
fn projection_to_resource_map_includes_func_parameter_decorators() {
    let projection = serde_json::json!({
        "functions": {
            "functions": {
                "entities": {
                    "fn-1": {
                        "id": "fn-1",
                        "name": "verify_dob",
                        "description": "Verify date of birth",
                        "code": "def verify_dob(conv: Conversation, dob: str):\n    return dob\n",
                        "parameters": [
                            {"id": "p1", "name": "dob", "description": "Date of birth, formatted as \"MM-DD-YYYY\"", "type": "string"}
                        ]
                    }
                }
            }
        }
    });
    let map = projection_to_resource_map(&projection).expect("map");
    let content = map
        .get("functions/verify_dob.py")
        .and_then(|r| r.payload.get("content"))
        .and_then(Value::as_str)
        .expect("function file content");
    assert!(content.contains("@func_description("));
    assert!(
        content.contains("@func_parameter('dob', 'Date of birth, formatted as \"MM-DD-YYYY\"')")
    );
    assert!(content.contains("def verify_dob("));
}

#[test]
fn projection_to_resource_map_uses_def_name_for_parameter_decorators() {
    let projection = serde_json::json!({
        "functions": {
            "functions": {
                "entities": {
                    "fn-1": {
                        "id": "fn-1",
                        "name": "Lookup Customer",
                        "description": "Look up a customer",
                        "code": "def helper(conv):\n    return None\n\n\ndef lookup_customer (conv: Conversation, customer_id: str):\n    return customer_id\n",
                        "parameters": [
                            {"id": "p1", "name": "customer_id", "description": "Customer id", "type": "string"}
                        ]
                    }
                }
            }
        }
    });
    let map = projection_to_resource_map(&projection).expect("map");
    let content = map
        .get("functions/lookup_customer.py")
        .and_then(|r| r.payload.get("content"))
        .and_then(Value::as_str)
        .expect("function file content");
    assert!(
        content.contains("@func_parameter('customer_id', 'Customer id')"),
        "missing customer_id decorator:\n{content}"
    );
    assert!(
        content.contains(
            "def helper(conv):\n    return None\n\n\n@func_description('Look up a customer')\n@func_parameter('customer_id', 'Customer id')\ndef lookup_customer ("
        ),
        "decorators were not inserted before lookup_customer:\n{content}"
    );
}

#[test]
fn projection_to_resource_map_includes_func_parameter_decorators_from_entities_shape() {
    let projection = serde_json::json!({
        "functions": {
            "functions": {
                "entities": {
                    "fn-1": {
                        "id": "fn-1",
                        "name": "verify_dob",
                        "description": "Verify date of birth",
                        "code": "def verify_dob(conv: Conversation, dob: str):\n    return dob\n",
                        "parameters": {
                            "entities": {
                                "p1": {
                                    "id": "p1",
                                    "name": "dob",
                                    "description": "Date of birth, formatted as \"MM-DD-YYYY\"",
                                    "type": "string"
                                }
                            },
                            "ids": ["p1"]
                        }
                    }
                }
            }
        }
    });
    let map = projection_to_resource_map(&projection).expect("map");
    let content = map
        .get("functions/verify_dob.py")
        .and_then(|r| r.payload.get("content"))
        .and_then(Value::as_str)
        .expect("function file content");
    assert!(content.contains("@func_description("));
    assert!(
        content.contains("@func_parameter('dob', 'Date of birth, formatted as \"MM-DD-YYYY\"')")
    );
    assert!(content.contains("def verify_dob("));
}

#[test]
fn projection_to_resource_map_orders_func_parameter_decorators_by_ids() {
    let projection = serde_json::json!({
        "functions": {
            "functions": {
                "entities": {
                    "fn-1": {
                        "id": "fn-1",
                        "name": "start_warm_transfer_flow",
                        "description": "Start warm transfer",
                        "code": "def start_warm_transfer_flow(conv: Conversation, handoff_reason: str, handoff_to: str):\n    return None\n",
                        "parameters": {
                            "entities": {
                                "p-handoff-to": {
                                    "id": "p-handoff-to",
                                    "name": "handoff_to",
                                    "description": "Destination queue",
                                    "type": "string"
                                },
                                "p-handoff-reason": {
                                    "id": "p-handoff-reason",
                                    "name": "handoff_reason",
                                    "description": "Why the transfer is needed",
                                    "type": "string"
                                }
                            },
                            "ids": ["p-handoff-reason", "p-handoff-to"]
                        }
                    }
                }
            }
        }
    });
    let map = projection_to_resource_map(&projection).expect("map");
    let content = map
        .get("functions/start_warm_transfer_flow.py")
        .and_then(|r| r.payload.get("content"))
        .and_then(Value::as_str)
        .expect("function file content");
    assert!(
        content.contains("@func_parameter('handoff_reason', 'Why the transfer is needed')"),
        "missing handoff_reason decorator:\n{content}"
    );
    assert!(
        content.contains("@func_parameter('handoff_to', 'Destination queue')"),
        "missing handoff_to decorator:\n{content}"
    );
}

#[test]
fn projection_to_resource_map_includes_func_latency_control_decorator() {
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
                            "enabled": true,
                            "initialDelay": 3,
                            "interval": 1,
                            "delayResponses": {
                                "entities": {
                                    "dr-1": {"message": "Please wait", "duration": 2}
                                },
                                "ids": ["dr-1"]
                            }
                        }
                    }
                }
            }
        }
    });
    let map = projection_to_resource_map(&projection).expect("map");
    let content = map
        .get("functions/slow_lookup.py")
        .and_then(|r| r.payload.get("content"))
        .and_then(Value::as_str)
        .expect("function file content");
    assert!(
        content.contains("@func_latency_control(delay_before_responses_start=3, silence_after_each_response=1, delay_responses=[('Please wait', 2)])"),
        "expected latency control decorator, got:\n{content}"
    );
}

#[test]
fn projection_to_resource_map_materializes_languages_and_translations() {
    let projection = serde_json::json!({
        "languages": {
            "defaultLanguageCode": "en-GB",
            "additionalLanguages": {
                "ids": ["lang-fr"],
                "entities": {
                    "lang-fr": {"code": "fr-FR"}
                }
            }
        },
        "translations": {
            "translations": {
                "ids": ["tn-1"],
                "entities": {
                    "tn-1": {
                        "id": "tn-1",
                        "translationKey": "greeting",
                        "translations": [
                            {"languageCode": "en-GB", "text": "Hello"},
                            {"languageCode": "fr-FR", "text": "Bonjour"}
                        ]
                    }
                }
            }
        }
    });

    let map = projection_to_resource_map(&projection).expect("projection resources");
    let languages = map
        .get("agent_settings/languages.yaml")
        .and_then(|resource| resource.payload.get("content"))
        .and_then(Value::as_str)
        .expect("languages content");
    assert!(languages.contains("default_language: en-GB"));
    assert!(languages.contains("- fr-FR"));

    let translations = map
        .get("config/translations.yaml")
        .and_then(|resource| resource.payload.get("content"))
        .and_then(Value::as_str)
        .expect("translations content");
    assert!(translations.contains("name: greeting"));
    assert!(translations.contains("en-GB: Hello"));
    assert!(translations.contains("fr-FR: Bonjour"));
}

#[test]
fn push_builder_emits_language_commands() {
    use adk_protobuf::command::Payload as CommandPayload;

    let mut resources = ResourceMap::new();
    resources.insert(
        "agent_settings/languages.yaml".to_string(),
        Resource {
            resource_id: "languages".to_string(),
            name: "languages".to_string(),
            file_path: "agent_settings/languages.yaml".to_string(),
            payload: serde_json::json!({
                "content": "default_language: en-GB\nadditional_languages:\n- fr-FR\n"
            }),
        },
    );
    let projection = serde_json::json!({
        "languages": {
            "defaultLanguageCode": "en-US",
            "additionalLanguages": {
                "ids": ["es-ES"],
                "entities": {
                    "es-ES": {"code": "es-ES"}
                }
            }
        }
    });

    let commands = build_push_commands(&resources, &projection);
    let types = commands
        .iter()
        .map(|command| command.r#type.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        types,
        vec![
            "languages_add_language",
            "languages_update_default_language",
            "languages_delete_language"
        ]
    );
    assert!(commands.iter().any(|command| {
        matches!(
            command.payload.as_ref(),
            Some(CommandPayload::LanguagesUpdateDefaultLanguage(payload))
                if payload.language_code == "en-GB"
        )
    }));
}

#[test]
fn push_builder_emits_translation_lifecycle_commands_by_translation_key() {
    use adk_protobuf::command::Payload as CommandPayload;

    let mut resources = ResourceMap::new();
    resources.insert(
        "config/translations.yaml".to_string(),
        Resource {
            resource_id: "translations".to_string(),
            name: "translations".to_string(),
            file_path: "config/translations.yaml".to_string(),
            payload: serde_json::json!({
                "content": "translations:\n- name: greeting\n  translations:\n    en-GB: Hello\n    fr-FR: Salut\n- name: farewell\n  translations:\n    en-GB: Bye\n    fr-FR: Au revoir\n"
            }),
        },
    );
    let projection = serde_json::json!({
        "translations": {
            "translations": {
                "ids": ["tn-greeting", "tn-old"],
                "entities": {
                    "tn-greeting": {
                        "id": "tn-greeting",
                        "translationKey": "greeting",
                        "translations": [
                            {"languageCode": "en-GB", "text": "Hello"},
                            {"languageCode": "fr-FR", "text": "Bonjour"}
                        ]
                    },
                    "tn-old": {
                        "id": "tn-old",
                        "translationKey": "old",
                        "translations": [
                            {"languageCode": "en-GB", "text": "Old"}
                        ]
                    }
                }
            }
        }
    });

    let commands = build_push_commands(&resources, &projection);
    let types = commands
        .iter()
        .map(|command| command.r#type.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        types,
        vec![
            "delete_translation",
            "create_translation",
            "update_translation"
        ]
    );
    assert!(commands.iter().any(|command| {
        matches!(
            command.payload.as_ref(),
            Some(CommandPayload::UpdateTranslation(payload))
                if payload.id == "tn-greeting"
                    && payload.translation_key.as_deref() == Some("greeting")
                    && payload.translations.iter().any(|entry| {
                        entry.language_code == "fr-FR"
                            && entry.text.as_deref() == Some("Salut")
                            && entry.is_auto_translated == Some(false)
                    })
        )
    }));
}

#[test]
fn push_builder_json_summaries_include_language_and_translation_payloads() {
    let mut resources = ResourceMap::new();
    resources.insert(
        "agent_settings/languages.yaml".to_string(),
        Resource {
            resource_id: "languages".to_string(),
            name: "languages".to_string(),
            file_path: "agent_settings/languages.yaml".to_string(),
            payload: serde_json::json!({
                "content": "default_language: en-GB\nadditional_languages:\n- fr-FR\n"
            }),
        },
    );
    resources.insert(
        "config/translations.yaml".to_string(),
        Resource {
            resource_id: "translations".to_string(),
            name: "translations".to_string(),
            file_path: "config/translations.yaml".to_string(),
            payload: serde_json::json!({
                "content": "translations:\n- name: greeting\n  translations:\n    en-GB: Hello\n    fr-FR: Salut\n- name: farewell\n  translations:\n    en-GB: Bye\n    fr-FR: Au revoir\n"
            }),
        },
    );
    let projection = serde_json::json!({
        "languages": {
            "defaultLanguageCode": "en-US",
            "additionalLanguages": {
                "ids": ["es-ES"],
                "entities": {
                    "es-ES": {"code": "es-ES"}
                }
            }
        },
        "translations": {
            "translations": {
                "ids": ["tn-greeting", "tn-old"],
                "entities": {
                    "tn-greeting": {
                        "id": "tn-greeting",
                        "translationKey": "greeting",
                        "translations": [
                            {"languageCode": "en-GB", "text": "Hello"},
                            {"languageCode": "fr-FR", "text": "Bonjour"}
                        ]
                    },
                    "tn-old": {
                        "id": "tn-old",
                        "translationKey": "old",
                        "translations": [
                            {"languageCode": "en-GB", "text": "Old"}
                        ]
                    }
                }
            }
        }
    });

    let summaries = build_push_commands(&resources, &projection)
        .iter()
        .map(command_to_json_summary)
        .collect::<Vec<_>>();
    let summary = |type_name: &str| {
        summaries
            .iter()
            .find(|summary| summary.get("type").and_then(Value::as_str) == Some(type_name))
            .unwrap_or_else(|| panic!("missing summary for {type_name}"))
    };

    assert_eq!(
        summary("languages_add_language")["languages_add_language"]["code"].as_str(),
        Some("fr-FR")
    );
    assert_eq!(
        summary("languages_update_default_language")["languages_update_default_language"]
            ["language_code"]
            .as_str(),
        Some("en-GB")
    );
    assert_eq!(
        summary("languages_delete_language")["languages_delete_language"]["code"].as_str(),
        Some("es-ES")
    );
    assert_eq!(
        summary("delete_translation")["delete_translation"]["id"].as_str(),
        Some("tn-old")
    );
    assert_eq!(
        summary("create_translation")["create_translation"]["translation_key"].as_str(),
        Some("farewell")
    );
    assert!(
        summary("create_translation")["create_translation"]["translations"]
            .as_array()
            .expect("create translations")
            .iter()
            .any(|translation| {
                translation["language_code"].as_str() == Some("fr-FR")
                    && translation["text"].as_str() == Some("Au revoir")
                    && translation["is_auto_translated"].as_bool() == Some(false)
            })
    );
    assert_eq!(
        summary("update_translation")["update_translation"]["id"].as_str(),
        Some("tn-greeting")
    );
    assert!(
        summary("update_translation")["update_translation"]["translations"]
            .as_array()
            .expect("update translations")
            .iter()
            .any(|translation| {
                translation["language_code"].as_str() == Some("fr-FR")
                    && translation["text"].as_str() == Some("Salut")
                    && translation["is_auto_translated"].as_bool() == Some(false)
            })
    );
}

#[test]
fn push_builder_orders_language_changes_around_translation_changes() {
    let mut resources = ResourceMap::new();
    resources.insert(
        "agent_settings/languages.yaml".to_string(),
        Resource {
            resource_id: "languages".to_string(),
            name: "languages".to_string(),
            file_path: "agent_settings/languages.yaml".to_string(),
            payload: serde_json::json!({
                "content": "default_language: en-GB\nadditional_languages:\n- fr-FR\n"
            }),
        },
    );
    resources.insert(
        "config/translations.yaml".to_string(),
        Resource {
            resource_id: "translations".to_string(),
            name: "translations".to_string(),
            file_path: "config/translations.yaml".to_string(),
            payload: serde_json::json!({
                "content": "translations:\n- name: greeting\n  translations:\n    en-GB: Hello\n    fr-FR: Bonjour\n"
            }),
        },
    );
    let projection = serde_json::json!({
        "languages": {
            "defaultLanguageCode": "en-US",
            "additionalLanguages": {
                "ids": ["es-ES"],
                "entities": {
                    "es-ES": {"code": "es-ES"}
                }
            }
        },
        "translations": {
            "translations": {
                "ids": [],
                "entities": {}
            }
        }
    });

    let commands = build_push_commands(&resources, &projection);
    let types = commands
        .iter()
        .map(|command| command.r#type.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        types,
        vec![
            "languages_add_language",
            "languages_update_default_language",
            "create_translation",
            "languages_delete_language"
        ]
    );
}

#[test]
fn changed_resource_push_suppresses_delayed_language_deletes() {
    let mut resources = ResourceMap::new();
    resources.insert(
        "agent_settings/languages.yaml".to_string(),
        Resource {
            resource_id: "languages".to_string(),
            name: "languages".to_string(),
            file_path: "agent_settings/languages.yaml".to_string(),
            payload: serde_json::json!({
                "content": "default_language: en-GB\nadditional_languages:\n- fr-FR\n"
            }),
        },
    );
    let projection = serde_json::json!({
        "languages": {
            "defaultLanguageCode": "en-US",
            "additionalLanguages": {
                "ids": ["es-ES"],
                "entities": {
                    "es-ES": {"code": "es-ES"}
                }
            }
        }
    });

    let full_push_commands = build_push_commands(&resources, &projection);
    let full_push_types = full_push_commands
        .iter()
        .map(|command| command.r#type.as_str())
        .collect::<Vec<_>>();
    assert!(full_push_types.contains(&"languages_delete_language"));

    let changed_resource_commands =
        build_push_commands_for_changed_resources(&resources, &projection, None);
    let changed_resource_types = changed_resource_commands
        .iter()
        .map(|command| command.r#type.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        changed_resource_types,
        vec![
            "languages_add_language",
            "languages_update_default_language",
        ]
    );
}

#[test]
fn translation_validation_checks_configured_language_coverage() {
    let mut resources = ResourceMap::new();
    resources.insert(
        "agent_settings/languages.yaml".to_string(),
        Resource {
            resource_id: "languages".to_string(),
            name: "languages".to_string(),
            file_path: "agent_settings/languages.yaml".to_string(),
            payload: serde_json::json!({
                "content": "default_language: en-GB\nadditional_languages:\n- fr-FR\n"
            }),
        },
    );
    resources.insert(
        "config/translations.yaml".to_string(),
        Resource {
            resource_id: "translations".to_string(),
            name: "translations".to_string(),
            file_path: "config/translations.yaml".to_string(),
            payload: serde_json::json!({
                "content": "translations:\n- name: greeting\n  translations:\n    en-GB: Hello\n    fr-FR: null\n    de-DE: Hallo\n"
            }),
        },
    );

    let errors = validate_language_translation_resources(&resources);
    let missing_error = errors
        .iter()
        .find(|error| error.contains("Missing translations for configured languages"))
        .expect("missing translation error");
    assert!(missing_error.contains("fr-FR"));
    assert!(
        errors
            .iter()
            .any(|error| error.contains("Translation for language not configured"))
    );
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
    let commands = build_push_commands(&resources, &projection);
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
    let commands = build_push_commands(&resources, &projection);
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
    let commands = build_push_commands(&resources, &projection);
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
fn projection_to_resource_map_includes_singleton_and_aggregate_files() {
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
    assert!(handoff_content.contains("outbound_endpoint: trunk"));
    assert!(handoff_content.contains("outbound_encryption: tls"));
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
fn rules_references_from_projection_accepts_camel_and_snake_global_functions() {
    let cases = [
        (
            serde_json::json!({
                "agentSettings": {
                    "rules": {
                        "references": {
                            "sms": {"sms-1": true},
                            "handoff": {"ho-1": false},
                            "attributes": {"attr-1": true},
                            "globalFunctions": {"fn-camel": true},
                            "variables": {"var-1": true},
                            "translations": {"tr-1": false},
                        }
                    }
                }
            }),
            "fn-camel",
        ),
        (
            serde_json::json!({
                "agentSettings": {
                    "rules": {
                        "references": {
                            "global_functions": {"fn-snake": true},
                        }
                    }
                }
            }),
            "fn-snake",
        ),
    ];

    for (projection, function_id) in cases {
        let refs = rules_references_from_projection(&projection).expect("rules references");
        assert!(
            refs.global_functions
                .get(function_id)
                .copied()
                .unwrap_or(false)
        );
    }

    assert!(
        rules_references_from_projection(&serde_json::json!({
            "agentSettings": {"rules": {"references": {}}}
        }))
        .is_none()
    );
    assert!(rules_references_from_projection(&serde_json::json!({})).is_none());
}

#[test]
fn every_registered_resource_has_validation_parity_marker() {
    #[allow(clippy::disallowed_methods)]
    fn collect_rs_sources(dir: &std::path::Path, out: &mut String) {
        for entry in std::fs::read_dir(dir).expect("read source dir") {
            let path = entry.expect("source entry").path();
            if path.is_dir() {
                collect_rs_sources(&path, out);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                out.push_str(&std::fs::read_to_string(&path).expect("read source file"));
                out.push('\n');
            }
        }
    }

    let mut source = String::new();
    collect_rs_sources(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut source,
    );

    for descriptor in adk_types::RESOURCE_TYPE_REGISTRY {
        let marker = format!("Python {}.validate()", descriptor.type_name);
        let has_marker = source
            .lines()
            .any(|line| line.contains("Validation parity:") && line.contains(&marker));
        assert!(
            has_marker,
            "missing validation parity marker for {}",
            descriptor.type_name
        );
    }
}

#[test]
fn resource_parse_boundary_does_not_use_legacy_validate_yaml_name() {
    #[allow(clippy::disallowed_methods)]
    fn collect_rs_sources(dir: &std::path::Path, out: &mut String) {
        for entry in std::fs::read_dir(dir).expect("read source dir") {
            let path = entry.expect("source entry").path();
            if path.is_dir() {
                collect_rs_sources(&path, out);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                out.push_str(&std::fs::read_to_string(&path).expect("read source file"));
                out.push('\n');
            }
        }
    }

    let mut source = String::new();
    collect_rs_sources(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut source,
    );
    let legacy_name = ["validate", "local", "yaml"].join("_");

    assert!(
        !source.contains(&legacy_name),
        "resource-local parsing should use parse/error terminology, not {legacy_name}"
    );
}
