# Current TODOs

Short-term engineering notes for the next few refactor passes. Keep this file
small and current; historical parity context belongs in
`docs/python-adk-parity-history.md`.

## Resource Organization

Goal: keep `adk-core` focused on workflow orchestration while `adk-resources`
owns resource-family semantics.

Completed in `codex/resource-status-semantics`:

- Status snapshot payload and hash construction moved into `adk-resources`.
- Python function resource helpers used by status snapshots moved into
  `adk-resources`.
- `adk-core` still owns when snapshots are loaded/written, but no longer owns
  the per-resource rules for payload shape or hash normalization.

Remaining:

1. Consolidate Python function resource semantics.
   - Function parsing, decorator handling, status payload generation,
     materialization, validation, and command generation are still not fully
     grouped under a single resource-family module.
   - Move reusable function-resource behavior into `adk-resources`; leave only
     core workflow plumbing in `adk-core`.

2. Move cross-resource validation rules behind an `adk-resources` API.
   - Resource-local YAML validation already lives in `adk-resources`.
   - Flow/function/entity reference validation still lives in `adk-core`.
   - The target shape is for `adk-core` to call a resource validation entrypoint
     rather than host the resource rules itself.

3. Reorganize `adk-resources` by resource family.
   - Today it is still grouped by operation: `local_resources`,
     `materialization`, `command_gen`, and shared specs.
   - The longer-term shape should use durable resource-family modules such as
     `flows`, `functions`, `topics`, `agent_settings`, `api_integrations`, and
     `variants`.
   - The local layout taxonomy (`singletons`, `aggregates`,
     `per_resource_files`) should remain descriptive vocabulary, not the module
     boundary.

## Watchpoints

- Preserve Python ADK parity and replay coverage while moving code.
- Avoid changing generated project file formatting or status snapshot hashes
  unless that is the explicit goal of the change.
- Prefer small mechanical moves before semantic cleanup.
