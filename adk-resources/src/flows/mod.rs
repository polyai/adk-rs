//! Flow-family resource semantics.

pub(crate) mod command_gen;
mod discovery;
mod local;
mod materialization;
mod projection;
mod summary;
mod validation;

pub(crate) use command_gen::flow_resource_command_groups;
pub(crate) use discovery::{FlowConfig, FlowStep, FunctionStep};
pub(crate) use local::{FlowStepType, parse_flow_config_content, parse_flow_step_content};
pub(crate) use materialization::{flow_entries, insert_flow_resources};
pub(crate) use summary::payload_json_summary;
pub use validation::validate_flow_resources;
