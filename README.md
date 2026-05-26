# adk-rs (WIP Rust ADK)

[![Tests](https://github.com/polyai/adk-rs/actions/workflows/lint-and-test.yml/badge.svg)](https://github.com/polyai/adk-rs/actions/workflows/lint-and-test.yml)
[![Release](https://github.com/polyai/adk-rs/actions/workflows/release.yml/badge.svg)](https://github.com/polyai/adk-rs/actions/workflows/release.yml)

Rust implementation of PolyAI's Agent Development Kit CLI.

> [!NOTE]
> This repository is a work-in-progress Rust rewrite of the original Python ADK.
> New users should install [the Python ADK](https://github.com/polyai/adk), not this one.

## Using The CLI

The Rust CLI provides the `poly` command. It is intended to match the Python ADK command surface:

```bash
poly --help
poly init
poly pull
poly status
poly diff
poly validate
poly push
```

It can also be run from source while this rewrite is still in progress:

```bash
cargo run --bin poly -- --help
```

## System Dependencies

Some commands shell out to Python tooling:

- `ruff`: used by `poly format` for Python formatting.
- `ty`: used by `poly format --ty` for optional Python type checking.

## Repository Layout

- `adk-cli`: `poly` binary and CLI integration tests.
- `adk-core`: project workflows such as init, pull, push, status, diff, validate, chat, and deployments.
- `adk-resources`: resource-family semantics shared by core and push/pull workflows.
- `adk-push-pull`: push/pull orchestration while resource-specific logic migrates into `adk-resources`.
- `adk-api-client`: HTTP communication with the PolyAI backend.
- `adk-types`, `adk-io`, `adk-protobuf`, `adk-ffi`: shared support crates.

Contributor setup, release notes, and parity-test workflow live in
[`docs/development.md`](docs/development.md).
