# poly-adk-domain

Shared domain types used across the workspace.

## Responsibilities

- Canonical structs/enums for project config, status, resources, diffs, push results.
- Common error types and aliases used by higher layers.

Keep this crate small and dependency-light so it is easy to reuse from FFI/bindings.
