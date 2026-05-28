//! Local resource to platform command generation.

mod json_summary;
pub(crate) mod local_file_helpers;
mod queue;

pub use json_summary::command_to_json_summary;

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
}

impl CommandGroups {
    pub(crate) fn append(&mut self, other: CommandGroups) {
        self.deletes.extend(other.deletes);
        self.creates.extend(other.creates);
        self.updates.extend(other.updates);
        self.post_updates.extend(other.post_updates);
    }
}

pub fn build_push_commands(resources: &ResourceMap, projection: &Value) -> Vec<Command> {
    build_push_commands_with_actor(resources, projection, None)
}

pub fn try_build_push_commands(
    resources: &ResourceMap,
    projection: &Value,
) -> Result<Vec<Command>, CommandGenError> {
    try_build_push_commands_with_actor(resources, projection, None)
}

pub fn build_push_commands_with_actor(
    resources: &ResourceMap,
    projection: &Value,
    actor: Option<&str>,
) -> Vec<Command> {
    try_build_push_commands_with_actor(resources, projection, actor)
        .expect("valid local resources for command generation")
}

pub fn try_build_push_commands_with_actor(
    resources: &ResourceMap,
    projection: &Value,
    actor: Option<&str>,
) -> Result<Vec<Command>, CommandGenError> {
    build_push_commands_inner(resources, projection, actor, true)
}

pub fn build_push_commands_for_changed_resources(
    resources: &ResourceMap,
    projection: &Value,
    actor: Option<&str>,
) -> Vec<Command> {
    try_build_push_commands_for_changed_resources(resources, projection, actor)
        .expect("valid local resources for command generation")
}

pub fn try_build_push_commands_for_changed_resources(
    resources: &ResourceMap,
    projection: &Value,
    actor: Option<&str>,
) -> Result<Vec<Command>, CommandGenError> {
    build_push_commands_inner(resources, projection, actor, false)
}

fn build_push_commands_inner(
    resources: &ResourceMap,
    projection: &Value,
    actor: Option<&str>,
    include_deletes: bool,
) -> Result<Vec<Command>, CommandGenError> {
    let metadata = command_metadata_with_actor(actor);

    let groups = queue::resource_command_groups(resources, projection, &metadata)?;

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

fn command_metadata_with_actor(actor: Option<&str>) -> Option<Metadata> {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let created_by = actor
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(default_metadata_created_by);
    Some(Metadata {
        created_at: Some(prost_types::Timestamp {
            seconds: dur.as_secs() as i64,
            nanos: dur.subsec_nanos() as i32,
        }),
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
