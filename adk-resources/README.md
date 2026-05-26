# adk-resources

Resource-family semantics shared by ADK core workflows and push/pull orchestration.

## Responsibilities

- Own resource-family facts such as local file paths, projection paths, ID prefixes, and stable layout metadata.
- Grow toward resource-family modules that keep discovery, materialization, validation helpers, and command generation helpers together.
- Keep `adk-core` focused on workflow orchestration and `adk-push-pull` focused on push/pull orchestration.

This crate should not perform filesystem IO or HTTP requests.
