# adk-platform-api

API boundary crate between core workflows and remote platform operations.

## Responsibilities

- Defines the `PlatformClient` trait used by `adk-core`.
- Hosts adapter implementations (currently an in-memory client for local testing).

Future HTTP/protobuf-backed clients should implement this trait and live here.
