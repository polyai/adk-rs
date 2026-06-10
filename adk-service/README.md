# adk-service

API-aware orchestration layer for ADK workflows.

## Responsibilities

- Coordinate `adk-core` project/resource logic with `adk-api-client` transport.
- Preserve native CLI workflows for pull, push, diff, deployments, branches, chat, and conversations.
- Own status-file persistence decisions for native service flows until those behaviors are further split.

## Design Notes

- Keep HTTP/API-aware behavior here, not in `adk-core`.
- Keep command-line parsing and user output in `adk-cli`.
- Keep resource-family-specific semantics in `adk-resources`.
