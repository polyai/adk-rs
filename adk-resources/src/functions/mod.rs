//! Function-family resource semantics.
//!
//! This module collects the Python function behavior that used to be split
//! across operation-shaped modules: parsing/decorator helpers, legacy status
//! snapshot compatibility, projection materialization facts, and push command
//! generation.

pub(crate) mod command_gen;
mod discovery;
mod materialization;
mod parsing;
mod python;

pub(crate) use command_gen::{
    SpecialFunctionKind, function_entries, function_errors_update_from_projection,
    function_parameters_update_from_projection, function_raw_content,
    function_resource_command_groups, function_update_latency_control, infer_function_description,
    infer_function_parameters, latency_control_from_projection, local_function_name,
    local_latency_control_from_code, python_function_symbol, special_function_entry,
    special_function_name, variable_reference_ids_from_code,
};
pub(crate) use discovery::Function;
pub(crate) use materialization::insert_function_resources;
pub(crate) use parsing::{function_create_latency_control, try_function_code_from_local_content};
pub use python::{
    PYTHON_FLOW_IMPORT_STATUS_KEY_PREFIX, PYTHON_FUNCTION_STATUS_HASH_PREFIX,
    PythonDecoratorCallScan, extract_normalized_python_adk_decorators,
    function_parameter_decorator_names, function_signature_parameter_list,
    function_signature_parameters, insert_python_function_decorators, is_python_function_like_path,
    is_python_function_resource, legacy_python_function_raw, legacy_python_local_function_raw,
    legacy_python_snapshot_hashes, local_resource_content,
    normalize_legacy_python_status_function_resources, normalize_python_function_metadata_spacing,
    parse_python_string_args, raw_function_content, resource_file_content,
};
