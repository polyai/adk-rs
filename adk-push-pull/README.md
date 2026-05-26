# adk-push-pull

Push/pull orchestration while resource-family logic migrates into
`adk-resources`.

## Responsibilities

- Convert Agent Studio projections into local ADK resource maps, delegating resource-family facts to `adk-resources`.
- Generate Python-compatible protobuf command batches from local resources plus a remote projection, delegating reusable resource-family helpers to `adk-resources`.
- Provide JSON summaries used by dry-run/replay tests.

This crate does not perform filesystem IO or HTTP requests.

## Layout

- `materialization`: projection JSON to local resource maps.
- `command_gen`: local resource maps plus projection JSON to protobuf command batches.
- `function_parsing` and `yaml_resources`: shared helpers used by both directions.
