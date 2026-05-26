//! Resource-family semantics shared by core workflows and push/pull orchestration.
//!
//! This crate is the migration home for resource-specific behavior: local file
//! layout, projection paths, materialization facts, validation helpers, stable
//! IDs, and command generation helpers. `adk-core` should orchestrate workflows;
//! `adk-push-pull` should orchestrate push/pull.

pub mod ids;
pub mod projection;
pub mod specs;
