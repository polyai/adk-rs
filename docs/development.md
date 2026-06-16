# Development Guide

Notes for contributors working on the Rust ADK rewrite.

## Workspace Layout

- `adk-cli`: `poly` binary, CLI parsing, output, and integration tests.
- `adk-core`: project workflows such as init, pull, push, status, diff, validate, chat, and deployments.
- `adk-resources`: resource-family semantics such as discovery, local file layout, projection paths, materialization facts, validation helpers, stable IDs, and command generation helpers.
- `adk-api-client`: HTTP communication with PolyAI backend, plus in-memory implementation for testing.
- `adk-types`: shared data models and errors.
- `adk-io`: filesystem, hashing, diff, path, and serialization helpers.
- `adk-protobuf`: protobuf command definitions used by push.
- `adk-napi`: TypeScript N-API wrapper for in-memory push/pull workflows.
- `docs`: current TODOs, historical parity context, and testing strategy.

Each crate also has a short local README.

## Adding Or Updating Resource Types

Resource type metadata and behavior are intentionally split by responsibility:

- `adk-types/src/lib.rs` owns the central `RESOURCE_TYPE_REGISTRY`: Python class name, status resource key, ID prefix, and registry order.
- `adk-resources` is the home for resource-specific semantics: discovery, local file paths, projection extraction, materialization, validation helpers, typed lifecycle helpers, stable ID facts, and command generation helpers.
- `adk-core/src/validation.rs` owns validation orchestration plus cross-resource checks, such as flow step references, entity references, and flow-scoped function call-site rules. Resource-local validation helpers should live with the resource family.
- Push/pull orchestration should call `adk-resources` directly rather than adding a resource-specific intermediary crate.

Within `adk-resources`, top-level directories are reserved for ADK resource
families. Cross-resource orchestration such as discovery dispatch, push-command
queueing, command summaries, and shared input helpers should live as top-level
Rust modules/files unless they are nested inside a resource-family module.

When adding a Python ADK resource type to Rust:

1. Add one descriptor to `RESOURCE_TYPE_REGISTRY` in the same order as Python `RESOURCE_NAME_TO_CLASS`.
2. Add or update the matching resource-family module in `adk-resources` with discovery, local file layout, projection mapping, materialization, validation helpers, and push command facts for that family.
3. Register the type in the `adk-resources` discovery dispatch and keep discovery order aligned with the registry.
4. Add resource-local validation in the resource-family module when a single resource file or resource-owned collection can be checked in isolation.
5. Keep relationship checks in `validation.rs` when they need multiple resources.
6. Add or update push/pull command generation in `adk-resources` when the resource participates in pull/push behavior.
7. Extend parity coverage before or alongside behavior changes, especially for discovery order, validation output, file layout, and command generation.

### Local Resource File Taxonomy

Use these names consistently when describing a resource family's local file
layout:

- `singletons`: one local file represents one backend/config resource with its
  own command semantics, such as role, personality, ASR settings, channel
  configuration, safety filters, and rules.
- `aggregates`: one local file contains a list or map of peer backend resources,
  such as entities, variants, API integrations, SMS templates, handoffs,
  pronunciations, keyphrase boosting, and transcript corrections.
- `per_resource_files`: resource identity is represented by paths/files rather
  than entries inside an aggregate file. Topics, functions, variables, and flows
  belong in this family even when a resource family has child files or
  relationships spread across a directory tree.

Prefer these terms over older buckets such as "single-file" or "structured".
The taxonomy describes the local file layout, not whether a resource is typed or
whether the underlying payload is YAML, JSON, text, or Python. It should not be
the long-term module hierarchy; modules should be named for resource families
such as `agent_settings`, `api_integrations`, `flows`, or `topics`.

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

## Runtime Stub Templates

Rust ADK vendors Python `.pyi` helper stubs under `adk-core/python-gen-template` so
`poly init` and `poly pull` can populate project `_gen/` packages without a
runtime dependency on Python ADK.

These files are for local editor and type-checker support. User function code is
executed in the PolyAI Lambda runtime, where the real runtime modules are
provided by the platform, so the checked-in `_gen` files should not be treated as
a replacement local Python runtime.

The sync script is a uv script with inline metadata for its mypy `stubgen` and
Ruff dependencies, so no separate Python environment setup is needed. It follows
the Python ADK shape: `imports.json` selects the public runtime modules,
`stubgen` generates `.pyi` files, Ruff formats the generated tree, and Rust
generates `_gen/__init__.py` to re-export those types for user function imports.
If those public stubs import sibling runtime modules, the script also generates
support-only `.pyi` files so the stub graph remains resolvable without adding
new `_gen` exports.

To regenerate those templates from a local `genai_lambda_runtime` checkout:

```bash
uv run scripts/sync_runtime_gen_templates.py \
  --runtime-path ../genai_lambda_runtime/python/runtime
```

To check for drift without rewriting files:

```bash
uv run scripts/sync_runtime_gen_templates.py \
  --runtime-path ../genai_lambda_runtime/python/runtime \
  --check
```

## Releases

Binary releases are managed with `cargo-dist`. Tagged versions such as `v0.0.1`
build Linux and macOS archives plus a shell installer through GitHub Actions:

```bash
dist plan
git tag v0.0.1
git push origin v0.0.1
```

The release installer installs the `poly` binary and provides `adk` as a
packaging-time alias. Crates.io publishing is not implemented yet.

## Parity Tests

The main offline parity suite replays Python ADK recordings against the Rust CLI:

```bash
cargo test --test replay_python_adk_httpmock_fixtures_test
```

The `format-local` replay exercises both formatting and `--ty`, so it needs
the system dependencies from the README.

Recording refreshes are ignored by default because they call the real Agent
Studio API. To refresh them explicitly, run:

```bash
cargo test --test record_python_adk_from_manifest_test -- --ignored --nocapture
```

See `adk-cli/tests/fixtures/python-adk-recordings/README.md` for the recording
format and replay workflow, and `docs/parity-test-strategy.md` for when to use
recordings versus in-memory matrix tests.
