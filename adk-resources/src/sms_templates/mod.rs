//! SMS template resource-family semantics.

mod command_gen;
mod discovery;
mod materialization;

pub(crate) use command_gen::{payload_json_summary, sms_template_command_groups};
pub(crate) use discovery::{SMSTemplate, validate_local_yaml};
pub(crate) use materialization::insert_sms_template_resources;
