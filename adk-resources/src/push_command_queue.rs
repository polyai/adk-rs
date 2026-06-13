//! Resource command queue construction.
//!
//! Resource-family modules own command payload semantics. This module keeps the
//! cross-family command queue shape in one place so broad ordering choices stay
//! visible without reintroducing layout buckets as production module boundaries.

use crate::CommandGenError;
use crate::push_commands::CommandGroups;
use adk_protobuf::Metadata;
use adk_types::ResourceMap;
use serde_json::Value;

pub(crate) fn resource_command_groups(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> Result<CommandGroups, CommandGenError> {
    let mut groups = per_resource_command_groups(resources, projection, metadata)?;
    groups.append(aggregate_command_groups(resources, projection, metadata));
    groups.append(singleton_command_groups(resources, projection, metadata));
    Ok(groups)
}

fn per_resource_command_groups(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> Result<CommandGroups, CommandGenError> {
    let mut groups =
        crate::variables::variable_resource_command_groups(resources, projection, metadata);
    groups.append(crate::functions::function_resource_command_groups(
        resources, projection, metadata,
    )?);
    groups.append(crate::topics::topic_resource_command_groups(
        resources, projection, metadata,
    ));
    groups.append(crate::flows::flow_resource_command_groups(
        resources, projection, metadata,
    )?);
    Ok(groups)
}

fn aggregate_command_groups(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> CommandGroups {
    let mut groups = crate::entities::entity_command_groups(resources, projection, metadata);
    groups.append(crate::handoffs::handoff_command_groups(
        resources, projection, metadata,
    ));
    groups.append(crate::sms_templates::sms_template_command_groups(
        resources, projection, metadata,
    ));
    groups.append(crate::phrase_filters::phrase_filter_command_groups(
        resources, projection, metadata,
    ));

    let variant_lifecycle: crate::variants::VariantLifecycleCommands =
        crate::variants::variant_lifecycle_commands(resources, projection, metadata);
    let api_lifecycle: crate::api_integrations::ApiIntegrationLifecycleCommands =
        crate::api_integrations::api_integration_lifecycle_commands(
            resources, projection, metadata,
        );
    let keyphrase_lifecycle =
        crate::keyphrase_boosting::keyphrase_lifecycle_commands(resources, projection, metadata);
    let transcript_lifecycle = crate::transcript_corrections::transcript_lifecycle_commands(
        resources, projection, metadata,
    );
    let pronunciation_lifecycle =
        crate::pronunciations::pronunciation_lifecycle_commands(resources, projection, metadata);
    let language_lifecycle =
        crate::languages::additional_language_lifecycle_commands(resources, projection, metadata);
    let translation_lifecycle =
        crate::translations::translation_lifecycle_commands(resources, projection, metadata);

    groups.deletes.extend(variant_lifecycle.variant_deletes);
    groups.deletes.extend(api_lifecycle.integration_deletes);
    groups.deletes.extend(keyphrase_lifecycle.deletes);
    groups.deletes.extend(variant_lifecycle.attribute_deletes);
    groups.deletes.extend(transcript_lifecycle.deletes);
    groups.deletes.extend(pronunciation_lifecycle.deletes);
    groups.deletes.extend(translation_lifecycle.deletes);
    groups.deletes.extend(api_lifecycle.operation_deletes);

    groups.creates.extend(variant_lifecycle.variant_creates);
    groups.creates.extend(variant_lifecycle.attribute_creates);
    groups.creates.extend(api_lifecycle.integration_creates);
    groups.creates.extend(keyphrase_lifecycle.creates);
    groups.creates.extend(transcript_lifecycle.creates);
    groups.creates.extend(pronunciation_lifecycle.creates);
    groups.creates.extend(language_lifecycle.creates);
    crate::languages::append_default_language_update(
        &mut groups.creates,
        resources,
        projection,
        metadata,
    );
    groups.creates.extend(translation_lifecycle.creates);
    groups.creates.extend(api_lifecycle.operation_creates);

    groups.updates.extend(api_lifecycle.integration_updates);
    groups.updates.extend(variant_lifecycle.variant_updates);
    groups.updates.extend(variant_lifecycle.attribute_updates);
    groups.updates.extend(keyphrase_lifecycle.updates);
    groups.updates.extend(transcript_lifecycle.updates);
    groups.updates.extend(pronunciation_lifecycle.updates);
    groups.updates.extend(translation_lifecycle.updates);
    groups.updates.extend(language_lifecycle.updates);
    groups.updates.extend(api_lifecycle.operation_updates);
    groups.updates.extend(api_lifecycle.config_updates);
    groups.post_deletes.extend(language_lifecycle.deletes);

    groups
}

fn singleton_command_groups(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> CommandGroups {
    let remote_resources = crate::projection_to_resource_map(projection).unwrap_or_default();
    let mut groups = CommandGroups::default();

    crate::agent_settings::append_agent_settings_updates(
        &mut groups.updates,
        resources,
        &remote_resources,
        projection,
        metadata,
    );
    crate::experimental_config::append_experimental_config_update(
        &mut groups.updates,
        resources,
        projection,
        metadata,
    );
    crate::channels::append_channel_settings_updates(
        &mut groups.updates,
        resources,
        &remote_resources,
        metadata,
    );
    crate::asr_settings::append_asr_settings_update(
        &mut groups.updates,
        resources,
        &remote_resources,
        metadata,
    );

    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::local_resource;
    use adk_protobuf::channels::channel_update_status::ChannelStatus as ChannelUpdateStatusKind;
    use adk_protobuf::channels::{ChannelStatus, ChannelType};
    use adk_protobuf::command::Payload as CommandPayload;

    fn flatten(groups: CommandGroups) -> Vec<adk_protobuf::Command> {
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
    fn singleton_files_emit_real_update_commands() {
        let mut resources = ResourceMap::new();
        resources.insert(
            "agent_settings/personality.yaml".to_string(),
            local_resource(
                "agent_settings/personality.yaml",
                "personality",
                "adjectives:\n  Polite: true\ncustom: Recording parity custom personality.\n",
            ),
        );
        resources.insert(
            "agent_settings/role.yaml".to_string(),
            local_resource(
                "agent_settings/role.yaml",
                "role",
                "value: CustomerServiceRepresentative\nadditional_info: Recording parity role detail.\ncustom: ''\n",
            ),
        );
        resources.insert(
            "agent_settings/safety_filters.yaml".to_string(),
            local_resource(
                "agent_settings/safety_filters.yaml",
                "safety_filters",
                "enabled: true\ncategories:\n  violence:\n    enabled: true\n    level: medium\n",
            ),
        );
        resources.insert(
            "voice/configuration.yaml".to_string(),
            local_resource(
                "voice/configuration.yaml",
                "voice_configuration",
                r#"
greeting:
  welcome_message: Hello from tests.
  language_code: en-US
style_prompt:
  prompt: Keep it compact.
disclaimer_messages:
  enabled: true
  message: This call may be recorded.
  language_code: en-US
"#,
            ),
        );
        resources.insert(
            "voice/speech_recognition/asr_settings.yaml".to_string(),
            local_resource(
                "voice/speech_recognition/asr_settings.yaml",
                "asr_settings",
                "barge_in: true\ninteraction_style: balanced\n",
            ),
        );
        resources.insert(
            "voice/safety_filters.yaml".to_string(),
            local_resource(
                "voice/safety_filters.yaml",
                "voice_safety_filters",
                "enabled: true\ncategories:\n  violence:\n    enabled: true\n    level: medium\n",
            ),
        );
        resources.insert(
            "chat/configuration.yaml".to_string(),
            local_resource(
                "chat/configuration.yaml",
                "chat_configuration",
                r#"
greeting:
  welcome_message: Hello from chat.
  language_code: en-US
style_prompt:
  prompt: Keep webchat compact.
"#,
            ),
        );
        resources.insert(
            "chat/safety_filters.yaml".to_string(),
            local_resource(
                "chat/safety_filters.yaml",
                "chat_safety_filters",
                "enabled: true\ncategories:\n  hate:\n    enabled: true\n    level: medium\n",
            ),
        );

        let commands = flatten(singleton_command_groups(
            &resources,
            &serde_json::json!({}),
            &None,
        ));
        let types = commands
            .iter()
            .map(|command| command.r#type.as_str())
            .collect::<Vec<_>>();
        for expected in [
            "update_personality",
            "update_role",
            "update_content_filter_settings",
            "channel_update_greeting",
            "channel_update_style_prompt",
            "channel_update_safety_filters",
            "channel_update_status",
            "voice_channel_update_disclaimer",
            "voice_channel_update_asr_settings",
        ] {
            assert!(
                types.contains(&expected),
                "missing singleton update command: {expected}"
            );
        }

        let asr = commands
            .iter()
            .find(|command| command.r#type == "voice_channel_update_asr_settings")
            .expect("voice_channel_update_asr_settings command");
        match &asr.payload {
            Some(CommandPayload::VoiceChannelUpdateAsrSettings(payload)) => {
                let settings = payload.asr_settings.as_ref().expect("asr settings");
                assert_eq!(settings.barge_in, Some(true));
                assert_eq!(
                    settings
                        .latency_config
                        .as_ref()
                        .map(|config| config.interaction_style.as_str()),
                    Some("balanced")
                );
            }
            _ => panic!("unexpected payload variant for voice_channel_update_asr_settings command"),
        }

        let webchat_greeting = commands
            .iter()
            .find(|command| {
                command.r#type == "channel_update_greeting"
                    && matches!(
                        command.payload.as_ref(),
                        Some(CommandPayload::ChannelUpdateGreeting(payload))
                            if payload.channel_type == ChannelType::WebChat as i32
                    )
            })
            .expect("webchat greeting update");
        match &webchat_greeting.payload {
            Some(CommandPayload::ChannelUpdateGreeting(payload)) => {
                assert_eq!(payload.channel_type, ChannelType::WebChat as i32);
                assert_eq!(
                    payload
                        .greeting
                        .as_ref()
                        .and_then(|greeting| greeting.welcome_message.as_deref()),
                    Some("Hello from chat.")
                );
            }
            _ => panic!("unexpected payload variant for webchat greeting command"),
        }

        assert!(commands.iter().any(|command| {
            command.r#type == "channel_update_safety_filters"
                && matches!(
                    command.payload.as_ref(),
                    Some(CommandPayload::ChannelUpdateSafetyFilters(payload))
                        if payload.channel_type == ChannelType::Voice as i32
                )
        }));
        assert!(commands.iter().any(|command| {
            command.r#type == "channel_update_safety_filters"
                && matches!(
                    command.payload.as_ref(),
                    Some(CommandPayload::ChannelUpdateSafetyFilters(payload))
                        if payload.channel_type == ChannelType::WebChat as i32
                )
        }));

        let webchat_status = commands
            .iter()
            .find(|command| command.r#type == "channel_update_status")
            .expect("webchat channel status update");
        match &webchat_status.payload {
            Some(CommandPayload::ChannelUpdateStatus(payload)) => match payload.channel_status {
                Some(ChannelUpdateStatusKind::Webchat(webchat)) => {
                    assert_eq!(webchat.status, ChannelStatus::Created as i32);
                }
                _ => panic!("unexpected channel status payload"),
            },
            _ => panic!("unexpected payload variant for webchat channel status command"),
        }
    }

    #[test]
    fn aggregate_files_emit_real_create_commands() {
        let mut resources = ResourceMap::new();
        resources.insert(
            "config/variant_attributes.yaml".to_string(),
            local_resource(
                "config/variant_attributes.yaml",
                "variant_attributes",
                r#"
variants:
  - name: default
  - name: treatment
attributes:
  - name: adk-recording-cohort
    values:
      default: control
      treatment: treatment
"#,
            ),
        );
        resources.insert(
            "config/api_integrations.yaml".to_string(),
            local_resource(
                "config/api_integrations.yaml",
                "api_integrations",
                r#"
api_integrations:
  - name: adk_recording_api
    description: Recording-only API integration.
    environments:
      sandbox:
        base_url: https://example.invalid/sandbox
        auth_type: none
    operations:
      - name: get_recording_status
        method: GET
        resource: /status
  - name: adk_recording_api_backup
    description: Recording-only backup API integration.
    operations:
      - name: get_recording_status
        method: GET
        resource: /backup/status
"#,
            ),
        );
        resources.insert(
            "voice/speech_recognition/keyphrase_boosting.yaml".to_string(),
            local_resource(
                "voice/speech_recognition/keyphrase_boosting.yaml",
                "keyphrase_boosting",
                "keyphrases:\n  - keyphrase: ADK parity\n    level: boosted\n",
            ),
        );
        resources.insert(
            "voice/speech_recognition/transcript_corrections.yaml".to_string(),
            local_resource(
                "voice/speech_recognition/transcript_corrections.yaml",
                "transcript_corrections",
                r#"
corrections:
  - name: ADK spelling
    description: Correct ADK spelling.
    regular_expressions:
      - regular_expression: agent development kid
        replacement: agent development kit
        replacement_type: full
"#,
            ),
        );
        resources.insert(
            "voice/response_control/pronunciations.yaml".to_string(),
            local_resource(
                "voice/response_control/pronunciations.yaml",
                "pronunciations",
                r#"
pronunciations:
  - regex: \bADK\b
    replacement: Agent Development Kit
    case_sensitive: true
    language_code: en-US
"#,
            ),
        );

        let commands = flatten(aggregate_command_groups(
            &resources,
            &serde_json::json!({}),
            &None,
        ));
        let types = commands
            .iter()
            .map(|command| command.r#type.as_str())
            .collect::<Vec<_>>();
        for expected in [
            "variant_create_variant",
            "variant_create_attribute",
            "create_api_integration",
            "create_api_integration_operation",
            "create_keyphrase_boosting",
            "create_transcript_corrections",
            "pronunciations_create_pronunciation",
        ] {
            assert!(
                types.contains(&expected),
                "missing aggregate-file create command: {expected}"
            );
        }

        let attribute = commands
            .iter()
            .find(|command| command.r#type == "variant_create_attribute")
            .expect("variant_create_attribute command");
        match &attribute.payload {
            Some(CommandPayload::VariantCreateAttribute(payload)) => {
                let values = payload
                    .variant_values
                    .as_ref()
                    .expect("variant values")
                    .values
                    .values()
                    .cloned()
                    .collect::<Vec<_>>();
                assert!(values.contains(&"control".to_string()));
                assert!(values.contains(&"treatment".to_string()));
            }
            _ => panic!("unexpected payload variant for variant_create_attribute command"),
        }

        let api = commands
            .iter()
            .find(|command| command.r#type == "create_api_integration")
            .expect("create_api_integration command");
        match &api.payload {
            Some(CommandPayload::CreateApiIntegration(payload)) => {
                assert_eq!(payload.name, "adk_recording_api");
                assert_eq!(
                    payload
                        .environments
                        .as_ref()
                        .and_then(|envs| envs.sandbox.as_ref())
                        .map(|env| env.base_url.as_str()),
                    Some("https://example.invalid/sandbox")
                );
            }
            _ => panic!("unexpected payload variant for create_api_integration command"),
        }

        let operation_ids = commands
            .iter()
            .filter(|command| command.r#type == "create_api_integration_operation")
            .filter_map(|command| match &command.payload {
                Some(CommandPayload::CreateApiIntegrationOperation(payload)) => {
                    Some(payload.id.as_str())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(operation_ids.len(), 2);
        assert_ne!(operation_ids[0], operation_ids[1]);
    }
}
