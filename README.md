# adk-rs

[![Tests](https://github.com/polyai/adk-rs/actions/workflows/lint-and-test.yml/badge.svg)](https://github.com/polyai/adk-rs/actions/workflows/lint-and-test.yml)

Workspace for the Rust port of PolyAI's ADK. The goal is feature parity with the
Python ADK while making it easier to test and ship the CLI, as well as embed the tooling itself.

## Layout

- `adk-cli`: `poly` / `adk` binaries, CLI parsing, output, and integration tests.
- `adk-core`: project workflows such as init, pull, push, status, diff, validate, chat, and deployments.
- `adk-platform-api`: HTTP communication with PolyAI backend (plus in-memory implementation for testing).
- `adk-domain`: shared domain models and errors.
- `adk-io`: filesystem, hashing, diff, path, and serialization helpers.
- `adk-protobuf`: protobuf command definitions used by push.
- `adk-ffi`: thin FFI-facing wrappers for future library bindings.
- `docs`: compatibility matrix, release gate, and remaining parity TODOs.

Each crate also has a short local README.

## Common Commands

```bash
cargo check --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p adk-cli --bin poly -- --help
```

## Parity Tests

The main offline parity suite replays Python ADK recordings against the Rust CLI:

```bash
cargo test -p adk-cli --test replay_python_adk_httpmock_fixtures_test
```

Recording refreshes are ignored by default because they call the real Agent
Studio API:

```bash
cargo test -p adk-cli --test record_python_adk_from_manifest_test -- --ignored --nocapture
```

See `adk-cli/tests/fixtures/python-adk-recordings/README.md` for the recording
format and replay workflow.
