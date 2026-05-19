//! Local resource to platform command generation.

pub(crate) mod flows;
pub(crate) mod functions;
mod json_summary;
pub(crate) mod single_file_resources;
pub(crate) mod topics;
pub(crate) mod variables;

pub use json_summary::command_to_json_summary;

use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::{Command, Metadata};
use adk_types::ResourceMap;
use serde_json::Value;
use uuid::Uuid;

pub fn build_phase1_commands(resources: &ResourceMap, projection: &Value) -> Vec<Command> {
    build_phase1_commands_with_actor(resources, projection, None)
}

pub fn build_phase1_commands_with_actor(
    resources: &ResourceMap,
    projection: &Value,
    actor: Option<&str>,
) -> Vec<Command> {
    build_phase1_commands_inner(resources, projection, actor, true)
}

pub fn build_phase1_commands_for_changed_resources(
    resources: &ResourceMap,
    projection: &Value,
    actor: Option<&str>,
) -> Vec<Command> {
    build_phase1_commands_inner(resources, projection, actor, false)
}

fn build_phase1_commands_inner(
    resources: &ResourceMap,
    projection: &Value,
    actor: Option<&str>,
    include_deletes: bool,
) -> Vec<Command> {
    let metadata = command_metadata_with_actor(actor);

    let flow_groups = flows::flow_resource_command_groups(resources, projection, &metadata);
    let function_groups =
        functions::function_resource_command_groups(resources, projection, &metadata);
    let topic_groups = topics::topic_resource_command_groups(resources, projection, &metadata);
    let variable_groups =
        variables::variable_resource_command_groups(resources, projection, &metadata);
    let single_file_groups = single_file_resources::single_file_resource_command_groups(
        resources, projection, &metadata,
    );
    let single_file_resources::CommandGroups {
        deletes: variable_deletes,
        creates: variable_creates,
        updates: variable_updates,
        post_updates: variable_post_updates,
    } = variable_groups;

    let mut deletes: Vec<Command> = if include_deletes {
        variable_deletes
            .into_iter()
            .chain(function_groups.deletes)
            .chain(topic_groups.deletes)
            .chain(flow_groups.deletes)
            .chain(single_file_groups.deletes)
            .collect()
    } else {
        Vec::new()
    };
    if include_deletes {
        order_commands_with_priority(&mut deletes, DELETE_COMMAND_PRIORITY);
    }

    let mut creates: Vec<Command> = variable_creates
        .into_iter()
        .chain(function_groups.creates)
        .chain(topic_groups.creates)
        .chain(flow_groups.creates)
        .chain(single_file_groups.creates)
        .collect();
    order_commands_with_priority(&mut creates, CREATE_COMMAND_PRIORITY);

    let mut updates: Vec<Command> = variable_updates
        .into_iter()
        .chain(function_groups.updates)
        .chain(topic_groups.updates)
        .chain(flow_groups.updates)
        .chain(single_file_groups.updates)
        .collect();
    order_commands_with_priority(&mut updates, UPDATE_COMMAND_PRIORITY);

    let mut out: Vec<Command> = Vec::new();
    out.extend(deletes);
    out.extend(creates);
    out.extend(updates);
    out.extend(variable_post_updates);
    out.extend(function_groups.post_updates);
    out.extend(topic_groups.post_updates);
    out.extend(flow_groups.post_updates);
    out.extend(single_file_groups.post_updates);
    out
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
