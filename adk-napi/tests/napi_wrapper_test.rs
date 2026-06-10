use adk_napi::{NapiPullInput, NapiPushInput, pull, push};
use adk_protobuf::CommandBatch;
use prost::Message;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

type FileMap = BTreeMap<String, String>;

const ROOT: &str = "project";

#[test]
fn pull_materializes_multiple_resource_families() {
    let mut files = FileMap::new();
    files.insert("README.md".to_string(), "notes\n".to_string());
    files.insert(adk_core::STATUS_FILE.to_string(), "ignored".to_string());

    let output = pull(NapiPullInput {
        root: ROOT.to_string(),
        files,
        pull_projection_json: cross_section_projection().to_string(),
        base_projection_json: None,
        force: None,
    })
    .expect("pull output");

    assert_eq!(output.files.get("README.md"), Some(&"notes\n".to_string()));
    assert!(!output.files.contains_key(adk_core::STATUS_FILE));
    assert_contains(&output.files, "topics/billing.yaml", "Remote billing");
    assert_contains(
        &output.files,
        "functions/lookup_account.py",
        "lookup_account",
    );
    assert_contains(&output.files, "config/entities.yaml", "CustomerId");
    assert_contains(&output.files, "config/handoffs.yaml", "Front Desk");
    assert_contains(
        &output.files,
        "config/sms_templates.yaml",
        "appointment_reminder",
    );
    assert_contains(
        &output.files,
        "voice/response_control/phrase_filtering.yaml",
        "Block Competitor",
    );
    assert_contains(&output.files, "_gen/decorators.py", "func_description");
    assert!(output.conflicts.is_empty());
    assert!(has_write(&output, "topics/billing.yaml"));
    assert!(has_write(&output, "config/entities.yaml"));
}

#[test]
fn pull_reports_conflicts_against_base_projection() {
    let base_projection = topic_projection("billing", "Base remote");
    let updated_projection = topic_projection("billing", "Updated remote");
    let first_pull = pull(NapiPullInput {
        root: ROOT.to_string(),
        files: FileMap::new(),
        pull_projection_json: base_projection.to_string(),
        base_projection_json: None,
        force: None,
    })
    .expect("first pull output");
    let mut files = first_pull.files;
    let local_edit = "name: billing\nenabled: true\nactions: \"\"\ncontent: \"Local edit\"\nexample_queries: []\n";
    files.insert("topics/billing.yaml".to_string(), local_edit.to_string());

    let output = pull(NapiPullInput {
        root: ROOT.to_string(),
        files,
        pull_projection_json: updated_projection.to_string(),
        base_projection_json: Some(base_projection.to_string()),
        force: None,
    })
    .expect("pull output");

    assert_eq!(output.conflicts, vec!["topics/billing.yaml".to_string()]);
    assert_eq!(
        output.files.get("topics/billing.yaml"),
        Some(&local_edit.to_string())
    );
    assert!(!has_write(&output, "topics/billing.yaml"));
}

#[test]
fn push_returns_command_batch_for_multiple_resource_families() {
    let output = push(NapiPushInput {
        root: ROOT.to_string(),
        files: push_file_map(),
        projection_json: "{}".to_string(),
        last_known_sequence: 7,
        created_by: Some("tester@example.com".to_string()),
        current_time: Some("2026-06-10T12:34:56.123Z".to_string()),
        force: None,
        skip_validation: Some(true),
    })
    .expect("push output");

    assert!(output.success);
    assert_eq!(output.message, None);
    let batch = decode_batch(output);
    assert_eq!(batch.last_known_sequence, 7);

    let command_types = batch
        .commands
        .iter()
        .map(|command| command.r#type.as_str())
        .collect::<BTreeSet<_>>();
    for expected in [
        "create_function",
        "create_topic",
        "entity_create",
        "handoff_create",
        "sms_create_template",
        "stop_keywords_create",
    ] {
        assert!(
            command_types.contains(expected),
            "expected command type {expected}, got {command_types:?}"
        );
    }
    assert!(batch.commands.iter().all(|command| {
        command
            .metadata
            .as_ref()
            .is_some_and(|metadata| metadata.created_by == "tester@example.com")
    }));
}

#[test]
fn rejects_invalid_file_map_paths_through_public_pull_api() {
    let err = expect_napi_err(pull(NapiPullInput {
        root: ROOT.to_string(),
        files: BTreeMap::from([("../escape.txt".to_string(), "nope".to_string())]),
        pull_projection_json: "{}".to_string(),
        base_projection_json: None,
        force: None,
    }));

    assert!(err.reason.contains("INVALID_INPUT"));
}

#[test]
fn rejects_invalid_projection_json() {
    let err = expect_napi_err(pull(NapiPullInput {
        root: ROOT.to_string(),
        files: FileMap::new(),
        pull_projection_json: "{nope".to_string(),
        base_projection_json: None,
        force: None,
    }));

    assert!(err.reason.contains("INVALID_PROJECTION"));
}

#[test]
fn rejects_negative_last_known_sequence() {
    let err = expect_napi_err(push(NapiPushInput {
        root: ROOT.to_string(),
        files: FileMap::new(),
        projection_json: "{}".to_string(),
        last_known_sequence: -1,
        created_by: None,
        current_time: None,
        force: None,
        skip_validation: None,
    }));

    assert!(err.reason.contains("INVALID_INPUT"));
}

fn expect_napi_err<T>(result: napi::Result<T>) -> napi::Error {
    match result {
        Ok(_) => panic!("expected N-API call to fail"),
        Err(error) => error,
    }
}

fn assert_contains(files: &FileMap, path: &str, needle: &str) {
    let content = files
        .get(path)
        .unwrap_or_else(|| panic!("expected file {path}"));
    assert!(
        content.contains(needle),
        "expected {path} to contain {needle:?}, got:\n{content}"
    );
}

fn has_write(output: &adk_napi::NapiPullOutput, path: &str) -> bool {
    output
        .changes
        .iter()
        .any(|change| change.kind == "write" && change.path == path)
}

fn decode_batch(output: adk_napi::NapiPushOutput) -> CommandBatch {
    let bytes = output
        .command_batch_bytes
        .expect("successful push should return command batch bytes");
    CommandBatch::decode(bytes.as_ref()).expect("decode command batch")
}

fn push_file_map() -> FileMap {
    BTreeMap::from([
        (
            "topics/sample.yaml".to_string(),
            "name: sample\nenabled: true\nactions: \"\"\ncontent: \"hello\"\nexample_queries: []\n"
                .to_string(),
        ),
        (
            "functions/lookup_account.py".to_string(),
            "@func_description('Lookup account')\ndef lookup_account(conv: Conversation):\n    return {'local': True}\n"
                .to_string(),
        ),
        (
            "config/entities.yaml".to_string(),
            "entities:\n- name: CustomerId\n  description: Local entity\n  entity_type: free_text\n  config: {}\n"
                .to_string(),
        ),
        (
            "config/handoffs.yaml".to_string(),
            "handoffs:\n- name: Front Desk\n  description: Local handoff\n  is_default: false\n  sip_config:\n    method: refer\n    phone_number: '+200'\n  sip_headers: []\n"
                .to_string(),
        ),
        (
            "config/sms_templates.yaml".to_string(),
            "sms_templates:\n- name: appointment_reminder\n  text: Local reminder\n  env_phone_numbers:\n    sandbox: ''\n    pre_release: ''\n    live: '+200'\n"
                .to_string(),
        ),
        (
            "voice/response_control/phrase_filtering.yaml".to_string(),
            "phrase_filtering:\n- name: Block Competitor\n  description: Local phrase filter\n  regular_expressions:\n  - '\\\\bcompetitor\\\\b'\n  say_phrase: true\n  language_code: en-US\n"
                .to_string(),
        ),
    ])
}

fn cross_section_projection() -> Value {
    json!({
        "knowledgeBase": {
            "topics": {
                "entities": {
                    "topic-1": {
                        "id": "topic-1",
                        "name": "billing",
                        "isActive": true,
                        "actions": "",
                        "content": "Remote billing",
                        "exampleQueries": []
                    }
                }
            }
        },
        "functions": {
            "functions": {
                "entities": {
                    "function-1": {
                        "id": "function-1",
                        "name": "lookup_account",
                        "description": "Lookup account",
                        "code": "def lookup_account(conv: Conversation):\n    return {'remote': True}\n",
                        "archived": false
                    }
                }
            }
        },
        "entities": {
            "entities": {
                "entities": {
                    "entity-1": {
                        "id": "entity-1",
                        "name": "CustomerId",
                        "description": "Remote entity",
                        "type": "FreeText",
                        "config": {}
                    }
                }
            }
        },
        "handoff": {
            "handoffs": {
                "entities": {
                    "handoff-1": {
                        "id": "handoff-1",
                        "name": "Front Desk",
                        "description": "Remote handoff",
                        "sipConfig": {
                            "config": {
                                "$case": "refer",
                                "value": { "phoneNumber": "+15551234567" }
                            }
                        },
                        "sipHeaders": { "headers": [] },
                        "active": true,
                        "isDefault": false
                    }
                }
            }
        },
        "sms": {
            "templates": {
                "entities": {
                    "sms-1": {
                        "id": "sms-1",
                        "name": "appointment_reminder",
                        "text": "Remote reminder",
                        "envPhoneNumbers": {
                            "sandbox": "",
                            "preRelease": "",
                            "live": "+15550000013"
                        },
                        "active": true
                    }
                }
            }
        },
        "stopKeywords": {
            "filters": {
                "entities": {
                    "phrase-1": {
                        "id": "phrase-1",
                        "title": "Block Competitor",
                        "description": "Remote phrase filter",
                        "regularExpressions": ["\\\\bcompetitor\\\\b"],
                        "sayPhrase": true,
                        "languageCode": "en-US"
                    }
                }
            }
        }
    })
}

fn topic_projection(name: &str, content: &str) -> Value {
    json!({
        "knowledgeBase": {
            "topics": {
                "entities": {
                    format!("topic-{name}"): {
                        "id": format!("topic-{name}"),
                        "name": name,
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
