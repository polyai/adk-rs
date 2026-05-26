# adk-resources

Resource-family semantics shared by ADK core workflows and push/pull orchestration.

## Responsibilities

- Own resource-family facts such as local file paths, projection paths, ID prefixes, and stable layout metadata.
- Keep discovery, materialization, validation helpers, stable ID helpers, and command generation helpers close to the resource semantics they describe.
- Keep `adk-core` focused on workflow orchestration and `adk-api-client` focused on transport.

Filesystem access in this crate should go through `adk-io`. This crate should not perform HTTP requests.
