//! Handoff resource-family semantics.

mod command_gen;
mod discovery;
mod local;
mod materialization;

pub(crate) use command_gen::{handoff_command_groups, payload_json_summary};
pub(crate) use discovery::Handoff;
pub(crate) use materialization::insert_handoff_resources;
