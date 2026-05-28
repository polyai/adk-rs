//! Push commands for resource families represented by one file per resource.
//!
//! Flows, functions, topics, and variables use the local path as part of the
//! resource identity. Some of these families have child files or derived
//! resources, but they are still organized around one resource path at a time
//! rather than a shared aggregate YAML file.

use super::CommandGroups;
use crate::CommandGenError;
use adk_protobuf::Metadata;
use adk_protobuf::command::Payload as CommandPayload;
use adk_types::ResourceMap;
use serde_json::Value;

pub(crate) fn per_resource_file_command_groups(
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

pub(crate) fn payload_json_summary(payload: &CommandPayload) -> Option<(&'static str, Value)> {
    crate::flows::payload_json_summary(payload)
}
