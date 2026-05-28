# Current TODOs

Short-term engineering notes for the next few refactor passes. Keep this file
small and current; historical parity context belongs in
`docs/python-adk-parity-history.md`.

## Resource Organization

Goal: keep `adk-core` focused on workflow orchestration while `adk-resources`
owns resource-family semantics.

Current state:

- Status snapshot payload and hash construction moved into `adk-resources`.
- Python function resource helpers used by status snapshots moved into
  `adk-resources`.
- `adk-core` still owns when snapshots are loaded/written, but no longer owns
  the per-resource rules for payload shape or hash normalization.
- Push command generation entrypoints use the current `build_push_commands`
  naming; the old `phase1` terminology has been removed.
- Resource-specific tests have been moved closer to their implementation for
  functions, flows, singleton files, aggregate files, status snapshots, and
  related materialization helpers. `adk-resources/src/tests.rs` is now reserved
  mostly for cross-resource orchestration and broad projection coverage.
- The first durable function-family module exists at `adk-resources/src/functions`.
  It owns function discovery, Python parsing/decorator helpers, legacy status
  compatibility helpers, projection materialization, validation helpers, and
  push command generation.

Remaining:

1. Move cross-resource validation rules behind an `adk-resources` API.
   - Resource-local YAML validation already lives in `adk-resources`.
   - Function-local Python syntax, decorator annotation, and flow-scoped
     signature validation now live in `adk-resources`.
   - Flow/function/entity reference validation still lives in `adk-core`.
   - The target shape is for `adk-core` to call a resource validation entrypoint
     rather than host the resource rules itself.

2. Reorganize `adk-resources` by resource family.
   - Today it is still grouped by operation: `local_resources`,
     `materialization`, `command_gen`, and shared specs.
   - The longer-term shape should use durable resource-family modules such as
     `flows`, `functions`, `topics`, `agent_settings`, `api_integrations`, and
     `variants`.
   - `functions` is now the pilot module for this shape; repeat the pattern for
     flows and other resource families.
   - The local layout taxonomy (`singletons`, `aggregates`,
     `per_resource_files`) should remain descriptive vocabulary, not the module
     boundary.
   - Use the colocated tests as a guide for the production module boundaries:
     resource-family semantics should move together, while broad orchestration
     tests stay in umbrella modules.

## Watchpoints

- Preserve Python ADK parity and replay coverage while moving code.
- Avoid changing generated project file formatting or status snapshot hashes
  unless that is the explicit goal of the change.
- Prefer small mechanical moves before semantic cleanup.
