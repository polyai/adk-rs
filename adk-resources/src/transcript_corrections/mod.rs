//! Transcript correction resource-family semantics.

mod command_gen;
mod discovery;
mod materialization;

pub(crate) use command_gen::{
    regular_expression_json, transcript_correction_json, transcript_lifecycle_commands,
};
pub(crate) use discovery::TranscriptCorrection;
pub(crate) use materialization::insert_transcript_correction_resources;
