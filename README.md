# adk-rs

Rust port of the ADK Python CLI and core library.

## Workspace Layout

- `adk-cli`: CLI surface (`poly` / `adk`) and command dispatch.
- `adk-core`: project orchestration and high-level workflows.
- `adk-domain`: shared data models and error types.
- `adk-io`: hashing, diffs, path normalization, serialization helpers.
- `adk-platform-api`: platform/API client boundary traits and adapters.
- `adk-ffi`: FFI-facing wrappers for future Python/TypeScript bindings.
- `docs`: compatibility notes and release gate docs.

## Quick Start

- Build: `cargo check --workspace`
- Test: `cargo test --workspace`
- Run CLI: `cargo run -p adk-cli --bin poly -- --help`
