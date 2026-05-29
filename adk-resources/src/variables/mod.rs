//! Variable-family resource semantics.
//!
//! Variables are virtual local resources: discovery derives `variables/<name>`
//! paths from `conv.state.<name>` usage in Python functions, and push command
//! generation creates/deletes the corresponding remote variable records.

mod command_gen;
mod discovery;

pub(crate) use command_gen::variable_resource_command_groups;
pub(crate) use discovery::Variable;
