# adk-protobuf agent notes

- This crate is primarily generated protobuf Rust (`prost` output).
- TODO comments found in generated files are inherited from upstream `.proto` definitions.
- Treat those TODO comments as non-actionable in Rust parity audits unless there is a concrete
  runtime or feature failure tied to schema limitations.
- Prefer fixing schema-level TODOs in the upstream `.proto` source and regenerating bindings.
