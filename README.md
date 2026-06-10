![PolyAI](logo.png)

# adk-rs (Rust Rewrite of ADK)

[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Minimum rustc version](https://img.shields.io/badge/rustc-1.95.0%2B-lightgray.svg)](Cargo.toml)
[![GitHub release](https://img.shields.io/github/v/release/polyai/adk-rs?sort=semver&label=release)](https://github.com/polyai/adk-rs/releases)
[![Lint/Test](https://github.com/polyai/adk-rs/actions/workflows/lint-and-test.yml/badge.svg)](https://github.com/polyai/adk-rs/actions/workflows/lint-and-test.yml)
[![Coverage](https://github.com/polyai/adk-rs/actions/workflows/compute-coverage.yml/badge.svg)](https://github.com/polyai/adk-rs/actions/workflows/compute-coverage.yml)
[![Release workflow](https://github.com/polyai/adk-rs/actions/workflows/release.yml/badge.svg)](https://github.com/polyai/adk-rs/actions/workflows/release.yml)

A Rust implementation of the PolyAI Agent Development Kit CLI for managing
[Agent Studio](https://studio.us.poly.ai) projects locally. Like the Python ADK,
it provides a Git-like workflow for synchronizing project configuration between
your filesystem and Agent Studio.

**[ADK documentation](https://polyai.github.io/adk/)**

> [!NOTE]
> This repository is a work-in-progress Rust rewrite of the original
> [Python ADK](https://github.com/polyai/adk). New users should install the
> Python ADK unless they are explicitly testing the Rust port.

## Rust Port Status

This Rust CLI is intended to match the Python ADK command surface and on-disk
project layout. Most core local workflows are implemented, including start,
login, init, pull, push, status, diff, branch, format, validate, chat,
deployments, and project creation.

Some newer Python ADK features are still being ported, including conversations
commands. The `poly review` command group is present but still incomplete in
this Rust port.

## Early Access

The PolyAI ADK is currently in Early Access. Changes may land frequently while
the platform evolves. If you encounter an issue, first make sure you are running
the latest available version (try `poly self-update`).

## Prerequisites

Before using the ADK you must have:

- access to a PolyAI Agent Studio workspace

Run the guided setup to sign in, save an API key to
`~/.poly/credentials.json`, and optionally create a project:

```bash
poly start
```

You can also sign in without creating a project:

```bash
poly login
```

For automation, you can still provide an API key as an environment variable:

```bash
export POLY_ADK_KEY=<your-key>
```

Some commands also shell out to Python tooling:

- `ruff`: used by `poly format` for Python formatting
- `ty`: used by `poly format --ty` for optional Python type checking

## Installation

To try the Rust ADK from the latest GitHub release:

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/polyai/adk-rs/releases/latest/download/poly-adk-installer.sh | sh
```

The release installer installs the `poly` binary and provides `adk` as an alias.

## Usage

Once installed, use the `poly` command to manage Agent Studio projects:

```bash
poly start           # Sign in for the first time and optionally create a project
poly login           # Sign in and save an API key
poly init            # Initialize a local Agent Studio project
poly project create  # Create a new Agent Studio project
poly pull            # Pull latest configuration
poly push            # Push local changes
poly status          # View local project status
poly diff            # View local changes
poly revert          # Revert local changes
poly branch          # Manage branches
poly format          # Format resources
poly validate        # Validate configuration
poly deployments     # Manage deployments
poly chat            # Chat with the agent
poly docs            # Output reference documentation
```

Run:

```bash
poly --help
```

to see all available commands. Each command also supports `--help` for detailed
syntax:

```bash
poly push --help
```

## Commands

### `poly init`

Initialize an Agent Studio project locally. Runs interactively by default,
prompting for region, account, and project. You can also pass these directly:

```bash
poly init
poly init --region us-1 --account_id 123 --project_id my_project
poly init --base-path /path/to/projects
poly init --format
```

### `poly project create`

Create a new Agent Studio project under an account:

```bash
poly project create
poly project create --region us-1 --account_id 123 --name "My agent"
poly project create --greeting "Hello, how can I help you?"
```

### `poly pull`

Pull the latest project configuration from Agent Studio:

```bash
poly pull
poly pull --force
poly pull --format
```

### `poly push`

Push local changes to Agent Studio:

```bash
poly push
poly push --dry-run
poly push --skip-validation
poly push --force
poly push --format
```

### `poly status`

View changed, new, and deleted files in your project:

```bash
poly status
poly status --json
```

### `poly diff`

Show diffs between your local project and the remote version, or between named
versions:

```bash
poly diff
poly diff --files file1.yaml
poly diff abc1234
poly diff --before hash1 --after hash2
```

### `poly revert`

Revert local changes:

```bash
poly revert
poly revert file1.yaml file2.yaml
```

### `poly branch`

Manage branches:

```bash
poly branch list
poly branch current
poly branch create my-feature
poly branch switch my-feature
poly branch switch my-feature --force
poly branch delete my-feature
poly branch merge "Merge message"
```

### `poly format`

Format project resources. Python files are formatted with `ruff`, while YAML and
JSON resources are formatted in process. Use `--check` to report files that would
change without writing them; use `--ty` to also run type checking.

```bash
poly format
poly format --files file1.py
poly format --check
poly format --ty
```

### `poly validate`

Validate project configuration locally:

```bash
poly validate
poly validate --json
```

### `poly deployments`

List, inspect, promote, and roll back deployments:

```bash
poly deployments list
poly deployments list --env live --limit 20
poly deployments show abc1234
poly deployments promote --from abc1234 --to live
poly deployments rollback --to abc1234
```

### `poly chat`

Start an interactive chat session with your agent:

```bash
poly chat
poly chat --environment live
poly chat --channel webchat
poly chat --metadata
poly chat --message "Hello"
```

### `poly docs`

Output ADK reference documentation:

```bash
poly docs
poly docs --all
poly docs topics
poly docs --output doc_file.md
```

### `poly completion`

Generate shell completion scripts:

```bash
poly completion bash
poly completion zsh
poly completion fish
```

### `poly self-update`

Update a release-installer managed Rust ADK binary:

```bash
poly self-update
```

### EXPERIMENTAL: `poly uninstall`

Uninstall a shell-installed Rust ADK binary. This command reads
cargo-dist's install receipt, whose format is not a stable public API:

```bash
poly uninstall
```

## Repository Layout

- `adk-cli`: `poly` binary, CLI parsing, output, and integration tests
- `adk-core`: project workflows such as init, pull, push, status, diff,
  validate, chat, and deployments
- `adk-resources`: resource-family semantics shared by core and push/pull
  workflows
- `adk-api-client`: HTTP communication with the PolyAI backend
- `adk-types`, `adk-io`, `adk-protobuf`: shared support crates
- `adk-napi`: TypeScript N-API wrapper for in-memory push/pull workflows
- `docs`: parity notes, testing strategy, and contributor documentation

## Bugs & Feature Requests

Please report bugs or request features via
[GitHub Issues](https://github.com/polyai/adk-rs/issues).

## Contributing

See [docs/development.md](docs/development.md) for contributor setup, release
notes, and parity-test workflow.

## License

This project is licensed under the Apache License 2.0.
