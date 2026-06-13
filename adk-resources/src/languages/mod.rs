//! Language resource-family semantics.

mod command_gen;
mod discovery;
mod local;
mod materialization;

pub(crate) use command_gen::{
    additional_language_lifecycle_commands, append_default_language_update, payload_json_summary,
};
pub(crate) use discovery::{AdditionalLanguage, DefaultLanguage};
pub(crate) use local::language_codes_from_yaml;
pub(crate) use materialization::insert_language_resources;
