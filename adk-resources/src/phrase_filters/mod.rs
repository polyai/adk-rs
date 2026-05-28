//! Phrase filter resource-family semantics.

mod command_gen;
mod discovery;
mod materialization;

pub(crate) use command_gen::{payload_json_summary, phrase_filter_command_groups};
pub(crate) use discovery::PhraseFilter;
pub(crate) use materialization::insert_phrase_filter_resources;
