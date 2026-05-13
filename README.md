# adk-rs

[![Tests](https://github.com/polyai/adk-rs/actions/workflows/lint-and-test.yml/badge.svg)](https://github.com/polyai/adk-rs/actions/workflows/lint-and-test.yml)

Workspace for the Rust port of PolyAI's ADK. The goal is feature parity with the
Python ADK while making it easier to test and ship the CLI, as well as embed the tooling itself.

## Layout

- `adk-cli` (`poly-adk-cli`): `poly` binary, CLI parsing, output, and integration tests.
- `adk-core` (`poly-adk-core`): project workflows such as init, pull, push, status, diff, validate, chat, and deployments.
- `adk-platform-api` (`poly-adk-platform-api`): HTTP communication with PolyAI backend (plus in-memory implementation for testing).
- `adk-domain` (`poly-adk-domain`): shared domain models and errors.
- `adk-io` (`poly-adk-io`): filesystem, hashing, diff, path, and serialization helpers.
- `adk-protobuf` (`poly-adk-protobuf`): protobuf command definitions used by push.
- `adk-ffi` (`poly-adk-ffi`): thin FFI-facing wrappers for future library bindings.
- `docs`: remaining parity TODOs.

Each crate also has a short local README.

## System Dependencies

The full parity test suite expects these executables on `PATH`:

- `ruff`: used by `poly format` for Python formatting.
- `ty`: used by `poly format --ty` for optional Python type checking.

CI installs pinned standalone Astral binaries: `ruff 0.14.2` and `ty 0.0.20`.
Those versions match the current Python ADK lockfile/recording expectations.

## Common Commands

```bash
cargo check --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p poly-adk-cli --bin poly -- --help
```

## Parity Tests

The main offline parity suite replays Python ADK recordings against the Rust CLI:

```bash
cargo test -p poly-adk-cli --test replay_python_adk_httpmock_fixtures_test
```

The `format-local` replay exercises both formatting and `--ty`, so it needs
the system dependencies above.

Recording refreshes are ignored by default because they call the real Agent
Studio API:

```bash
cargo test -p poly-adk-cli --test record_python_adk_from_manifest_test -- --ignored --nocapture
```

See `adk-cli/tests/fixtures/python-adk-recordings/README.md` for the recording
format and replay workflow.
