//! Keyphrase boosting resource-family semantics.

mod command_gen;
mod discovery;
mod local;
mod materialization;

pub(crate) use command_gen::keyphrase_lifecycle_commands;
pub(crate) use discovery::KeyphraseBoosting;
pub(crate) use materialization::insert_keyphrase_boosting_resources;
