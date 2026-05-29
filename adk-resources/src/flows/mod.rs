//! Flow-family resource semantics.

pub(crate) mod command_gen;
mod discovery;
mod materialization;
mod validation;

pub(crate) use command_gen::flow_resource_command_groups;
pub(crate) use command_gen::payload_json_summary;
pub(crate) use discovery::{FlowConfig, FlowStep, FunctionStep};
pub(crate) use materialization::{flow_entries, insert_flow_resources};
pub use validation::validate_flow_resources;
