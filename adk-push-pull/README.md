# adk-push-pull

Projection materialization and push/pull command generation.

## Responsibilities

- Convert Agent Studio projections into local ADK resource maps.
- Generate Python-compatible protobuf command batches from local resources plus a remote projection.
- Provide JSON summaries used by dry-run/replay tests.

This crate does not perform filesystem IO or HTTP requests.

## Layout

- `materialization`: projection JSON to local resource maps.
- `command_gen`: local resource maps plus projection JSON to protobuf command batches.
- `function_parsing` and `yaml_resources`: shared helpers used by both directions.
