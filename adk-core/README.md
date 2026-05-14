# adk-core

Core orchestration layer for ADK workflows.

## Responsibilities

- Project initialization and config/status loading.
- Local resource discovery and status/diff computation.
- Pull/push/deployments workflow entrypoints.
- Coordination between domain models, IO helpers, and platform client traits.

## Design Notes

- Keep business logic here, not in `adk-cli`.
- Depend on abstractions from `adk-api-client`, not concrete network code.
