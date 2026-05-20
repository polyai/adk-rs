# Development Guide

Notes for contributors working on the Rust ADK rewrite.

## Workspace Layout

- `adk-cli`: `poly` binary, CLI parsing, output, and integration tests.
- `adk-core`: project workflows such as init, pull, push, status, diff, validate, chat, and deployments.
- `adk-push-pull`: projection materialization and push/pull command generation.
- `adk-api-client`: HTTP communication with PolyAI backend, plus in-memory implementation for testing.
- `adk-types`: shared data models and errors.
- `adk-io`: filesystem, hashing, diff, path, and serialization helpers.
- `adk-protobuf`: protobuf command definitions used by push.
- `adk-ffi`: thin FFI-facing wrappers for future library bindings.
- `docs`: parity TODOs and testing strategy.

Each crate also has a short local README.

## Common Commands

```bash
cargo check --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run --bin poly -- --help
```

CI uses pinned standalone Astral binaries for parity-sensitive formatting behavior:

- `ruff 0.14.2`
- `ty 0.0.35`

## Releases

Binary releases are managed with `cargo-dist`. Tagged versions such as `v0.0.1`
build Linux and macOS archives plus a shell installer through GitHub Actions:

```bash
dist plan
git tag v0.0.1
git push origin v0.0.1
```

The release installer installs the `poly` binary and also provides `adk` as a
packaging-time alias. Crates.io publishing is still a separate `cargo publish`
process.

## Parity Tests

The main offline parity suite replays Python ADK recordings against the Rust CLI:

```bash
cargo test --test replay_python_adk_httpmock_fixtures_test
```

The `format-local` replay exercises both formatting and `--ty`, so it needs
the system dependencies from the README.

A smaller direct Python-vs-Rust CLI parity test is also available. It is opt-in
so ordinary `cargo test` runs do not depend on whichever Python ADK happens to
be on `PATH`:

```bash
PYTHON_ADK_BIN=/path/to/python/poly cargo test --test python_adk_direct_cli_parity_test
```

Recording refreshes are ignored by default because they call the real Agent
Studio API:

```bash
cargo test --test record_python_adk_from_manifest_test -- --ignored --nocapture
```

See `adk-cli/tests/fixtures/python-adk-recordings/README.md` for the recording
format and replay workflow, and `docs/parity-test-strategy.md` for when to use
recordings versus in-memory matrix tests.
