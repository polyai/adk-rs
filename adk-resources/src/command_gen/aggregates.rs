//! Push commands for aggregate files.
//!
//! Aggregate files contain a list or map of peer backend resources in one local
//! file. Examples include entities, variants, API integrations, handoffs, SMS
//! templates, stop-keyword phrase filters, pronunciations, keyphrase boosting,
//! and transcript corrections.

mod summaries;

use super::CommandGroups;
use adk_protobuf::Metadata;
use adk_protobuf::command::Payload as CommandPayload;
use adk_types::ResourceMap;
use serde_json::Value;

pub(crate) fn aggregate_command_groups(
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

    groups.deletes.extend(variant_lifecycle.variant_deletes);
    groups.deletes.extend(api_lifecycle.integration_deletes);
    groups.deletes.extend(keyphrase_lifecycle.deletes);
    groups.deletes.extend(variant_lifecycle.attribute_deletes);
    groups.deletes.extend(transcript_lifecycle.deletes);
    groups.deletes.extend(pronunciation_lifecycle.deletes);
    groups.deletes.extend(api_lifecycle.operation_deletes);

    groups.creates.extend(variant_lifecycle.variant_creates);
    groups.creates.extend(variant_lifecycle.attribute_creates);
    groups.creates.extend(api_lifecycle.integration_creates);
    groups.creates.extend(keyphrase_lifecycle.creates);
    groups.creates.extend(transcript_lifecycle.creates);
    groups.creates.extend(pronunciation_lifecycle.creates);
    groups.creates.extend(api_lifecycle.operation_creates);

    groups.updates.extend(api_lifecycle.integration_updates);
    groups.updates.extend(variant_lifecycle.variant_updates);
    groups.updates.extend(variant_lifecycle.attribute_updates);
    groups.updates.extend(keyphrase_lifecycle.updates);
    groups.updates.extend(transcript_lifecycle.updates);
    groups.updates.extend(pronunciation_lifecycle.updates);
    groups.updates.extend(api_lifecycle.operation_updates);
    groups.updates.extend(api_lifecycle.config_updates);

    groups
}

pub(crate) fn payload_json_summary(payload: &CommandPayload) -> Option<(&'static str, Value)> {
    crate::entities::payload_json_summary(payload)
        .or_else(|| crate::handoffs::payload_json_summary(payload))
        .or_else(|| crate::sms_templates::payload_json_summary(payload))
        .or_else(|| crate::phrase_filters::payload_json_summary(payload))
        .or_else(|| summaries::payload_json_summary(payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::local_resource;

    fn flatten(groups: CommandGroups) -> Vec<adk_protobuf::Command> {
        groups
            .deletes
            .into_iter()
            .chain(groups.creates)
            .chain(groups.updates)
            .chain(groups.post_updates)
            .collect()
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
