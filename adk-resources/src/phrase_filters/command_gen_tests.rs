use super::*;
use adk_types::Resource;
use indexmap::IndexMap;
use std::collections::HashMap;

fn map_with(resources: Vec<(String, Resource)>) -> ResourceMap {
    let mut map: ResourceMap = IndexMap::new();
    for (path, resource) in resources {
        map.insert(path, resource);
    }
    map
}

fn flatten(groups: CommandGroups) -> Vec<Command> {
    groups
        .deletes
        .into_iter()
        .chain(groups.creates)
        .chain(groups.updates)
        .chain(groups.post_updates)
        .chain(groups.cleanup_deletes)
        .chain(groups.post_deletes)
        .collect()
}

#[test]
fn stop_keyword_payload_summaries_cover_optional_defaults_and_refs() {
    let mut global_functions = HashMap::new();
    global_functions.insert("fn-1".to_string(), true);
    let payloads = [
        (
            CommandPayload::StopKeywordsCreate(StopKeywordCreate {
                id: "sk-1".into(),
                title: "Stop".into(),
                description: "halt".into(),
                regular_expressions: vec!["stop".into()],
                say_phrase: true,
                references: Some(StopKeywordReferences { global_functions }),
                language_code: "en-US".into(),
            }),
            "stop_keywords_create",
            serde_json::json!({
                "id": "sk-1",
                "title": "Stop",
                "description": "halt",
                "regular_expressions": ["stop"],
                "say_phrase": true,
                "references": {"global_functions": {"fn-1": true}},
                "language_code": "en-US",
            }),
        ),
        (
            CommandPayload::StopKeywordsUpdate(StopKeywordUpdate {
                id: "sk-2".into(),
                title: None,
                description: Some("updated".into()),
                regular_expressions: vec![],
                say_phrase: Some(false),
                references: None,
                language_code: None,
            }),
            "stop_keywords_update",
            serde_json::json!({
                "id": "sk-2",
                "title": "",
                "description": "updated",
                "regular_expressions": [],
                "say_phrase": false,
                "references": {},
                "language_code": "",
            }),
        ),
        (
            CommandPayload::StopKeywordsDelete(StopKeywordDelete { id: "sk-3".into() }),
            "stop_keywords_delete",
            serde_json::json!({"id": "sk-3"}),
        ),
    ];

    for (payload, key, value) in payloads {
        assert_eq!(payload_json_summary(&payload), Some((key, value)));
    }
}

#[test]
fn stop_keyword_references_are_derived_from_function_field() {
    let pf_yaml = r#"
name: HangUp
description: end
regular_expressions:
  - "^bye$"
say_phrase: false
language_code: en-US
function: fn-one
"#;
    let resources = map_with(vec![(
        "voice/response_control/phrase_filtering.yaml/phrase_filtering/HangUp".into(),
        Resource {
            resource_id: "sk-hangup".into(),
            name: "HangUp".into(),
            file_path: "voice/response_control/phrase_filtering.yaml/phrase_filtering/HangUp"
                .into(),
            payload: serde_json::json!({ "content": pf_yaml }),
        },
    )]);
    let projection = serde_json::json!({});
    let commands = flatten(phrase_filter_command_groups(&resources, &projection, &None));
    let create = commands
        .iter()
        .find(|c| c.r#type == "stop_keywords_create")
        .expect("stop keyword create command");
    match &create.payload {
        Some(CommandPayload::StopKeywordsCreate(msg)) => {
            let refs = msg.references.as_ref().expect("references");
            assert_eq!(refs.global_functions.get("fn-one"), Some(&true));
        }
        _ => panic!("unexpected payload variant for stop keyword create command"),
    }
}

#[test]
fn phrase_filter_aggregate_file_parses_through_typed_model() {
    let pf_yaml = r#"
phrase_filtering:
  - name: HangUp
    description: " end "
    regular_expressions:
      - "^bye$"
    say_phrase: true
    language_code: en-US
    function: fn-one
  - name: Pause
    regular_expressions:
      - "^wait$"
"#;
    let resources = map_with(vec![(
        "voice/response_control/phrase_filtering.yaml".into(),
        Resource {
            resource_id: "phrase_filtering".into(),
            name: "phrase_filtering".into(),
            file_path: "voice/response_control/phrase_filtering.yaml".into(),
            payload: serde_json::json!({ "content": pf_yaml }),
        },
    )]);
    let projection = serde_json::json!({});
    let commands = flatten(phrase_filter_command_groups(&resources, &projection, &None));
    let creates = commands
        .iter()
        .filter_map(|command| match &command.payload {
            Some(CommandPayload::StopKeywordsCreate(create)) => Some(create),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(creates.len(), 2);
    let hangup = creates
        .iter()
        .find(|create| create.title == "HangUp")
        .expect("HangUp create command");
    assert_eq!(hangup.description, "end");
    assert_eq!(hangup.regular_expressions, vec!["^bye$"]);
    assert!(hangup.say_phrase);
    assert_eq!(hangup.language_code, "en-US");
    assert_eq!(
        hangup
            .references
            .as_ref()
            .and_then(|refs| refs.global_functions.get("fn-one")),
        Some(&true)
    );
}

#[test]
fn pulled_empty_phrase_filter_does_not_queue_delete() {
    let pf_yaml = r#"
phrase_filtering:
  - name: Empty
    description: ""
    regular_expressions: []
    say_phrase: false
    language_code: ""
"#;
    let resources = map_with(vec![(
        "voice/response_control/phrase_filtering.yaml".into(),
        Resource {
            resource_id: "phrase_filtering".into(),
            name: "phrase_filtering".into(),
            file_path: "voice/response_control/phrase_filtering.yaml".into(),
            payload: serde_json::json!({ "content": pf_yaml }),
        },
    )]);
    let projection = serde_json::json!({
        "stopKeywords": {
            "filters": {
                "entities": {
                    "sk-empty": {
                        "title": "Empty",
                        "description": "",
                        "regularExpressions": [],
                        "sayPhrase": false,
                        "languageCode": ""
                    }
                }
            }
        }
    });
    let commands = flatten(phrase_filter_command_groups(&resources, &projection, &None));

    assert!(
        !commands
            .iter()
            .any(|command| command.r#type == "stop_keywords_delete"),
        "pulled empty phrase filter should remain visible to command generation"
    );
    assert!(
        commands.is_empty(),
        "no-op pull/push should not queue commands"
    );
}

#[test]
fn phrase_filter_command_generation_fails_closed_when_local_aggregate_parse_fails() {
    let resources = map_with(vec![(
        "voice/response_control/phrase_filtering.yaml".into(),
        Resource {
            resource_id: "phrase_filtering".into(),
            name: "phrase_filtering".into(),
            file_path: "voice/response_control/phrase_filtering.yaml".into(),
            payload: serde_json::json!({
                "content": "phrase_filtering:\n  - name: HangUp\n    description: end\n    regular_expressions: '^bye$'\n    say_phrase: false\n    language_code: en-US\n"
            }),
        },
    )]);
    let projection = serde_json::json!({
        "stopKeywords": {
            "filters": {
                "entities": {
                    "sk-hangup": {
                        "title": "HangUp",
                        "description": "end",
                        "regularExpressions": ["^bye$"],
                        "sayPhrase": false,
                        "languageCode": "en-US"
                    }
                }
            }
        }
    });

    let groups = phrase_filter_command_groups(&resources, &projection, &None);
    assert!(flatten(groups).is_empty());
}

#[test]
fn stop_keywords_create() {
    let pf_yaml = r#"
name: HangUp
description: end
regular_expressions:
  - "^bye$"
say_phrase: false
language_code: en-US
"#;
    let resources = map_with(vec![(
        "voice/response_control/phrase_filtering.yaml/phrase_filtering/HangUp".into(),
        Resource {
            resource_id: "local".into(),
            name: "HangUp".into(),
            file_path: "voice/response_control/phrase_filtering.yaml/phrase_filtering/HangUp"
                .into(),
            payload: serde_json::json!({ "content": pf_yaml }),
        },
    )]);
    let projection = serde_json::json!({});
    let commands = flatten(phrase_filter_command_groups(&resources, &projection, &None));
    let types: Vec<&str> = commands.iter().map(|c| c.r#type.as_str()).collect();
    assert!(types.contains(&"stop_keywords_create"));
}
