# adk-api-client

API boundary crate between core workflows and remote platform operations.

## Responsibilities

- Defines the `PlatformClient` trait used by `adk-core`.
- Transports projection JSON, branch/deployment/chat payloads, and protobuf command batches.
- Hosts adapter implementations:
  - `HttpPlatformClient`: real Platform API integration (region/account/project scoped).
  - `InMemoryPlatformClient`: deterministic test double for unit/local tests.

## Push Support

The HTTP client implements real calls for projection, deployments, chat, branch,
and protobuf command-batch push endpoints. Projection materialization and
resource command generation live outside this crate: `adk-core` orchestrates
those flows by calling `adk-resources`, then asks this crate to send or fetch
API payloads.
