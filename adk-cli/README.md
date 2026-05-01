# adk-cli

Command-line interface crate for the Rust ADK port.

## Responsibilities

- Defines CLI parser shape and command tree.
- Exposes both executable names: `poly` and `adk`.
- Routes parsed args to `adk-core`.
- Handles user-facing output conventions (JSON vs human-readable).

## Key Paths

- `src/main.rs`: parser, dispatch, output/exit behavior.
- `src/bin/poly.rs`, `src/bin/adk.rs`: binary entrypoints.
- `tests/`: CLI surface and Python parity tests.
