use super::*;
use adk_protobuf::command::Payload as CommandPayload;
use adk_types::Resource;

#[test]
fn personality_update_filters_unknown_disabled_adjectives() {
    let mut resources = ResourceMap::new();
    resources.insert(
        AGENT_PERSONALITY_FILE.file_path.to_string(),
        Resource {
            resource_id: AGENT_PERSONALITY_FILE.resource_id.to_string(),
            name: AGENT_PERSONALITY_FILE.name.to_string(),
            file_path: AGENT_PERSONALITY_FILE.file_path.to_string(),
            payload: json!({
                "content": "adjectives:\n  Polite: true\n  RetiredAdjective: false\n  Calm: true\ncustom: ''\n"
            }),
        },
    );
    let mut commands = Vec::new();
    append_personality_update(&mut commands, &resources, &ResourceMap::new(), &None);

    let command = commands
        .iter()
        .find(|command| command.r#type == "update_personality")
        .expect("update_personality command");
    match &command.payload {
        Some(CommandPayload::UpdatePersonality(payload)) => {
            let adjectives = payload.adjectives.as_ref().expect("adjectives");
            assert_eq!(adjectives.values.get("Polite"), Some(&true));
            assert_eq!(adjectives.values.get("Calm"), Some(&true));
            assert!(!adjectives.values.contains_key("RetiredAdjective"));
        }
        _ => panic!("unexpected update_personality payload"),
    }
}
