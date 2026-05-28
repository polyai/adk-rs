//! Variant and variant-attribute resource-family semantics.

mod command_gen;
mod discovery;
mod materialization;

pub(crate) use command_gen::{
    VariantLifecycleCommands, attribute_references_json, attribute_values_json,
    variant_lifecycle_commands,
};
pub(crate) use discovery::{Variant, VariantAttribute, validate_local_yaml};
pub(crate) use materialization::insert_variant_resources;
