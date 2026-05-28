use super::*;
use adk_types::Resource;
use indexmap::IndexMap;

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
        .collect()
}

#[test]
fn stop_keyword_references_include_global_functions_map() {
    let pf_yaml = r#"
name: HangUp
description: end
regular_expressions:
  - "^bye$"
say_phrase: false
language_code: en-US
references:
  global_functions:
    fn-one: true
    fn-two: false
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
            assert_eq!(refs.global_functions.get("fn-two"), Some(&false));
        }
        _ => panic!("unexpected payload variant for stop keyword create command"),
    }
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
