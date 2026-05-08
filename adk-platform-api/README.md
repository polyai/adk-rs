# adk-platform-api

API boundary crate between core workflows and remote platform operations.

## Responsibilities

- Defines the `PlatformClient` trait used by `adk-core`.
- Hosts adapter implementations:
  - `HttpPlatformClient`: real Platform API integration (region/account/project scoped).
  - `InMemoryPlatformClient`: deterministic test double for unit/local tests.

## Push Support

The HTTP client implements real calls for projection, deployments, chat, branch,
and protobuf command-batch push endpoints. The push command builder covers the
phase-1 resources plus extended settings handled in `push_extended.rs`.
