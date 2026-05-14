# adk-cli

Command-line interface crate for the Rust ADK port.

## Responsibilities

- Defines CLI parser shape and command tree.
- Exposes the `poly` executable.
- Routes parsed args to `adk-core`.
- Handles user-facing output conventions (JSON vs human-readable).

## Key Paths

- `src/main.rs`: parser, dispatch, output/exit behavior.
- `src/bin/poly.rs`: binary entrypoint.
- `tests/`: CLI surface and Python parity tests.

## System Tools

`poly format` shells out to `ruff` for Python formatting. `poly format --ty`
also shells out to `ty check`. Keep both on `PATH` when running the full CLI
test suite.
