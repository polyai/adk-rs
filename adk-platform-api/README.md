# adk-platform-api

API boundary crate between core workflows and remote platform operations.

## Responsibilities

- Defines the `PlatformClient` trait used by `adk-core`.
- Hosts adapter implementations:
  - `HttpPlatformClient`: real Platform API integration (region/account/project scoped).
  - `InMemoryPlatformClient`: deterministic test double for unit/local tests.

## Current Caveat

The HTTP client implements real calls for projection/deployments/chat endpoints.
The push path currently uses a simplified command-batch payload and is marked in
code with a TODO for full protobuf command parity.
