//! Local resource to platform push-command construction.

use crate::CommandGenError;
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::{Command, Metadata};
use adk_types::ResourceMap;
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Default)]
pub(crate) struct CommandGroups {
    pub deletes: Vec<Command>,
    pub creates: Vec<Command>,
    pub updates: Vec<Command>,
    pub post_updates: Vec<Command>,
    /// Delete commands that clean up temporary resources created by this command batch.
    pub cleanup_deletes: Vec<Command>,
    pub post_deletes: Vec<Command>,
}

impl CommandGroups {
    pub(crate) fn append(&mut self, other: CommandGroups) {
        self.deletes.extend(other.deletes);
        self.creates.extend(other.creates);
        self.updates.extend(other.updates);
        self.post_updates.extend(other.post_updates);
        self.cleanup_deletes.extend(other.cleanup_deletes);
        self.post_deletes.extend(other.post_deletes);
    }
}

pub fn build_push_commands(resources: &ResourceMap, projection: &Value) -> Vec<Command> {
    build_push_commands_with_created_by(resources, projection, None)
}

pub fn try_build_push_commands(
    resources: &ResourceMap,
    projection: &Value,
) -> Result<Vec<Command>, CommandGenError> {
    try_build_push_commands_with_created_by(resources, projection, None)
}

pub fn build_push_commands_with_created_by(
    resources: &ResourceMap,
    projection: &Value,
    created_by_override: Option<&str>,
) -> Vec<Command> {
    try_build_push_commands_with_created_by(resources, projection, created_by_override)
        .expect("valid local resources for command generation")
}

pub fn try_build_push_commands_with_created_by(
    resources: &ResourceMap,
    projection: &Value,
    created_by_override: Option<&str>,
) -> Result<Vec<Command>, CommandGenError> {
    build_push_commands_inner(resources, projection, created_by_override, None, true)
}

/// Builds a full-resource command batch with caller-supplied command metadata.
///
/// This is intended for pure engine bindings that receive timestamp and author
/// context from their caller instead of reading environment variables or the
/// system clock at the API boundary. Existing CLI and API-client paths should
/// continue using the simpler wrappers unless they need deterministic metadata.
pub fn try_build_push_commands_with_metadata(
    resources: &ResourceMap,
    projection: &Value,
    created_by_override: Option<&str>,
    created_at_override: Option<prost_types::Timestamp>,
) -> Result<Vec<Command>, CommandGenError> {
    build_push_commands_inner(
        resources,
        projection,
        created_by_override,
        created_at_override,
        true,
    )
}

pub fn build_push_commands_for_changed_resources(
    resources: &ResourceMap,
    projection: &Value,
    created_by_override: Option<&str>,
) -> Vec<Command> {
    try_build_push_commands_for_changed_resources(resources, projection, created_by_override)
        .expect("valid local resources for command generation")
}

pub fn try_build_push_commands_for_changed_resources(
    resources: &ResourceMap,
    projection: &Value,
    created_by_override: Option<&str>,
) -> Result<Vec<Command>, CommandGenError> {
    build_push_commands_inner(resources, projection, created_by_override, None, false)
}

pub fn try_build_push_commands_for_changed_resources_with_metadata(
    resources: &ResourceMap,
    projection: &Value,
    created_by_override: Option<&str>,
    created_at_override: Option<prost_types::Timestamp>,
) -> Result<Vec<Command>, CommandGenError> {
    build_push_commands_inner(
        resources,
        projection,
        created_by_override,
        created_at_override,
        false,
    )
}

fn build_push_commands_inner(
    resources: &ResourceMap,
    projection: &Value,
    created_by_override: Option<&str>,
    created_at_override: Option<prost_types::Timestamp>,
    include_deletes: bool,
) -> Result<Vec<Command>, CommandGenError> {
    let metadata = command_metadata_with_created_by(created_by_override, created_at_override);

    let groups =
        crate::push_command_queue::resource_command_groups(resources, projection, &metadata)?;

    let mut deletes = if include_deletes {
        groups.deletes
    } else {
        Vec::new()
    };
    if include_deletes {
        order_commands_with_priority(&mut deletes, DELETE_COMMAND_PRIORITY);
    }

    let mut creates = groups.creates;
    order_commands_with_priority(&mut creates, CREATE_COMMAND_PRIORITY);

    let mut updates = groups.updates;
    order_commands_with_priority(&mut updates, UPDATE_COMMAND_PRIORITY);

    let mut out: Vec<Command> = Vec::new();
    out.extend(deletes);
    out.extend(creates);
    out.extend(updates);
    out.extend(groups.post_updates);
    out.extend(groups.cleanup_deletes);
    if include_deletes {
        out.extend(groups.post_deletes);
    }
    Ok(out)
}

const DELETE_COMMAND_PRIORITY: &[&str] = &[
    "variable_delete",
    "delete_start_function",
    "delete_end_function",
    "delete_function",
    "delete_flow_transition_function",
    "delete_topic",
    "handoff_delete",
    "sms_delete_template",
    "stop_keywords_delete",
    "entity_delete",
];

const CREATE_COMMAND_PRIORITY: &[&str] = &[
    "variable_create",
    "entity_create",
    "sms_create_template",
    "handoff_create",
    "create_start_function",
    "create_end_function",
    "create_function",
    "create_topic",
    "create_flow",
    "create_flow_transition_function",
    "create_step",
    "create_no_code_condition",
    "stop_keywords_create",
];

const UPDATE_COMMAND_PRIORITY: &[&str] = &[
    "variable_update",
    "entity_update",
    "update_rules",
    "update_start_function",
    "update_end_function",
    "update_function",
    "update_flow_transition_function",
    "update_topic",
    "sms_update_template",
    "handoff_update",
    "stop_keywords_update",
    "experimental_config_update_config",
];

fn order_commands_with_priority(commands: &mut [Command], priority: &[&str]) {
    commands.sort_by_key(|command| {
        priority
            .iter()
            .position(|value| *value == command.r#type.as_str())
            .unwrap_or(priority.len())
    });
}

fn command_metadata_with_created_by(
    created_by_override: Option<&str>,
    created_at_override: Option<prost_types::Timestamp>,
) -> Option<Metadata> {
    let created_at = created_at_override.unwrap_or_else(|| {
        let dur = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        prost_types::Timestamp {
            seconds: dur.as_secs() as i64,
            nanos: dur.subsec_nanos() as i32,
        }
    });
    let created_by = created_by_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(default_metadata_created_by);
    Some(Metadata {
        created_at: Some(created_at),
        created_by,
    })
}

pub(crate) fn default_metadata_created_by() -> String {
    "sdk-user".to_string()
}

pub(crate) fn push_command(
    out: &mut Vec<Command>,
    metadata: &Option<Metadata>,
    type_str: &str,
    payload: CommandPayload,
) {
    out.push(Command {
        r#type: type_str.to_string(),
        metadata: metadata.clone(),
        command_id: Uuid::new_v4().to_string(),
        payload: Some(payload),
    });
}
