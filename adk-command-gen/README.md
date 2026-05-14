# adk-command-gen

Pure projection materialization and push command generation.

## Responsibilities

- Convert Agent Studio projections into local ADK resource maps.
- Generate Python-compatible protobuf command batches from local resources plus a remote projection.
- Provide JSON summaries used by dry-run/replay tests.

This crate does not perform filesystem IO or HTTP requests.
