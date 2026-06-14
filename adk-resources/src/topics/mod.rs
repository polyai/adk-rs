//! Topic-family resource semantics.

mod command_gen;
mod discovery;
mod local;
mod materialization;

pub(crate) use command_gen::{topic_entries, topic_resource_command_groups};
pub(crate) use discovery::Topic;
pub(crate) use materialization::insert_topic_resources;
