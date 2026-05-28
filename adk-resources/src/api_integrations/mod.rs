//! API integration resource-family semantics.

mod command_gen;
mod discovery;
mod materialization;

pub(crate) use command_gen::environments_json;
pub(crate) use command_gen::{ApiIntegrationLifecycleCommands, api_integration_lifecycle_commands};
pub(crate) use discovery::ApiIntegration;
pub(crate) use materialization::insert_api_integration_resources;
