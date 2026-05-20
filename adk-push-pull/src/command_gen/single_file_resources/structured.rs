//! Push commands for structured single-file resource families such as variants, API integrations,
//! pronunciation data, transcript corrections, keyphrase boosting, and channel settings.

#[path = "structured/api_integrations.rs"]
mod api_integrations;
#[path = "structured/common.rs"]
mod common;
#[path = "structured/keyphrases.rs"]
mod keyphrases;
#[path = "structured/pronunciations.rs"]
mod pronunciations;
#[path = "structured/settings.rs"]
mod settings;
#[path = "structured/summaries.rs"]
mod summaries;
#[path = "structured/transcript_corrections.rs"]
mod transcript_corrections;
#[path = "structured/variants.rs"]
mod variants;

use super::CommandGroups;
use crate::projection_to_resource_map;
use adk_protobuf::Metadata;
use adk_protobuf::command::Payload as CommandPayload;
use adk_types::ResourceMap;
use serde_json::Value;

/// Builds push commands for structured one-file and multi-item resources.
///
/// This is the aggregation point for resources whose local representation is YAML
/// or JSON rather than Python code: variants, API integrations, ASR resources,
/// pronunciations, agent settings, channel configuration, and safety filters.
/// Each sub-family owns its detailed lifecycle comparison; this function combines
/// those lifecycles with single-file update checks and places the resulting
/// commands into the delete/create/update phases expected by push.
///
/// The comparison uses `projection_to_resource_map` as the remote baseline so the
/// command generator can distinguish real edits from materialization details such
/// as YAML key order or local file grouping.
pub(crate) fn structured_file_resource_command_groups(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> CommandGroups {
    let remote_resources = projection_to_resource_map(projection).unwrap_or_default();
    let mut groups = CommandGroups::default();
    let variant_lifecycle = variants::variant_lifecycle_commands(resources, projection, metadata);
    let api_lifecycle =
        api_integrations::api_integration_lifecycle_commands(resources, projection, metadata);
    let keyphrase_lifecycle =
        keyphrases::keyphrase_lifecycle_commands(resources, projection, metadata);
    let transcript_lifecycle =
        transcript_corrections::transcript_lifecycle_commands(resources, projection, metadata);
    let pronunciation_lifecycle =
        pronunciations::pronunciation_lifecycle_commands(resources, projection, metadata);

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

    settings::append_agent_settings_updates(
        &mut groups.updates,
        resources,
        &remote_resources,
        metadata,
    );
    settings::append_channel_settings_updates(
        &mut groups.updates,
        resources,
        &remote_resources,
        metadata,
    );

    groups
}

pub(crate) fn payload_json_summary(payload: &CommandPayload) -> Option<(&'static str, Value)> {
    summaries::payload_json_summary(payload)
}
