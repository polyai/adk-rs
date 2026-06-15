use super::*;
use adk_protobuf::command::Payload as CommandPayload;
use adk_types::Resource;

fn content_resource(spec: crate::specs::FileResourceSpec, content: &str) -> Resource {
    Resource {
        resource_id: spec.resource_id.to_string(),
        name: spec.name.to_string(),
        file_path: spec.file_path.to_string(),
        payload: json!({ "content": content }),
    }
}

#[test]
fn personality_update_filters_unknown_disabled_adjectives() {
    let mut resources = ResourceMap::new();
    resources.insert(
        AGENT_PERSONALITY_FILE.file_path.to_string(),
        content_resource(
            AGENT_PERSONALITY_FILE,
            "adjectives:\n  Polite: true\n  RetiredAdjective: false\n  Curious: true\n  Calm: true\ncustom: Recording parity custom personality.\n",
        ),
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
            assert!(!adjectives.values.contains_key("Curious"));
            assert_eq!(
                payload.custom.as_deref(),
                Some("Recording parity custom personality.")
            );
        }
        _ => panic!("unexpected update_personality payload"),
    }
}

#[test]
fn role_update_parses_local_content_through_typed_model() {
    let mut resources = ResourceMap::new();
    resources.insert(
        AGENT_ROLE_FILE.file_path.to_string(),
        content_resource(
            AGENT_ROLE_FILE,
            "value: other\nadditional_info: Front desk\ncustom: Concierge\n",
        ),
    );
    let mut commands = Vec::new();
    append_role_update(&mut commands, &resources, &ResourceMap::new(), &None);

    let command = commands
        .iter()
        .find(|command| command.r#type == "update_role")
        .expect("update_role command");
    match &command.payload {
        Some(CommandPayload::UpdateRole(payload)) => {
            assert_eq!(payload.value.as_deref(), Some("other"));
            assert_eq!(payload.additional_info.as_deref(), Some("Front desk"));
            assert_eq!(payload.custom.as_deref(), Some("Concierge"));
        }
        _ => panic!("unexpected update_role payload"),
    }
}

#[test]
fn safety_filter_update_parses_local_content_through_typed_model() {
    let mut resources = ResourceMap::new();
    resources.insert(
        AGENT_SAFETY_FILTERS_FILE.file_path.to_string(),
        content_resource(
            AGENT_SAFETY_FILTERS_FILE,
            r#"
categories:
  violence:
    enabled: true
    level: strict
  hate:
    enabled: false
    level: medium
  sexual:
    enabled: false
    level: lenient
  self_harm:
    enabled: false
    level: medium
"#,
        ),
    );
    let mut commands = Vec::new();
    append_safety_filter_update(&mut commands, &resources, &ResourceMap::new(), &None);

    let command = commands
        .iter()
        .find(|command| command.r#type == "update_content_filter_settings")
        .expect("update_content_filter_settings command");
    match &command.payload {
        Some(CommandPayload::UpdateContentFilterSettings(payload)) => {
            assert_eq!(payload.r#type.as_deref(), Some("azure"));
            assert_eq!(payload.disabled, Some(false));
            let violence = payload
                .azure_config
                .as_ref()
                .and_then(|config| config.violence.as_ref())
                .expect("violence category");
            assert!(violence.is_active);
            assert_eq!(violence.precision, "STRICT");
        }
        _ => panic!("unexpected update_content_filter_settings payload"),
    }
}
