//! Pronunciation resource-family semantics.

mod command_gen;
mod discovery;
mod materialization;

pub(crate) use command_gen::pronunciation_lifecycle_commands;
pub(crate) use discovery::Pronunciation;
pub(crate) use materialization::insert_pronunciation_resources;
