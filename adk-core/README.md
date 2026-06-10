# adk-core

Core project and resource logic for ADK workflows.

## Responsibilities

- Project initialization and local config/status helpers.
- Filesystem-generic resource collection, validation, formatting helpers, and push command planning.
- Projection/resource diff and materialization support shared by native and embedded callers.

## Design Notes

- Keep pure project/resource logic here, not in `adk-cli`.
- Do not depend on `adk-api-client`; API-aware orchestration belongs in `adk-service`.
- Keep resource-family-specific semantics in `adk-resources`.
