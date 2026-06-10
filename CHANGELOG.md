# Changelog

Concise user-facing release notes for the Rust ADK CLI.

## Unreleased

## 0.0.10-rc.0 - 2026-06-10

- Added the `@poly-ai/adk-node` N-API package for TypeScript services to run pure pull and push workflows against in-memory file maps.
- Split API-aware orchestration into `poly-adk-service` while keeping reusable push/pull planning in `poly-adk-core`.
- Added manual npm publishing with Linux, macOS Intel, macOS Apple Silicon, and Windows native addon builds.
- Added release-version guardrails so the Rust workspace and npm package stay in lock-step.

## 0.0.9 - 2026-06-09

- Added `poly conversations list`, `poly conversations get`, and `poly conversations get-audio` backed by the Developer API.
- Added language and translation resource parity, including validation and push/pull support.
- Improved flow creation parity for function-step starts and added webchat config validation plus webchat enablement on push.
- Fixed release-binary docs lookup, symlinked resource traversal, review parser validation, and project creation prompt parity.

## 0.0.8 - 2026-06-02

- Added `poly uninstall` for shell-installed ADK releases.
- Aligned push authorship override behavior with Python ADK by using `ADK_COMMAND_USER_OVERRIDE` for request headers and command metadata.
- Fixed resource update parity for personality adjectives, experimental config IDs, and empty function parameter updates.

## 0.0.7 - 2026-06-01

- Added `poly start` and `poly login` onboarding with Auth0 sign-in, credential-file API key storage, and guided project creation.
- Added Studio project creation parity so `poly start` lets Agent Studio generate project IDs.
- Improved release-facing documentation for Rust ADK setup and current command coverage.

## 0.0.6 - 2026-05-29

- Improved Python function parsing robustness with AST-backed decorator and signature handling.
- Consolidated resource discovery, materialization, validation, status, and command generation semantics around resource-family modules.
- Expanded parity coverage for resource command generation, API-client named pull resolution, and CLI command flows.
- Improved CRAP risk reporting by excluding generated protobuf code and caching the baseline report in CI.

## 0.0.5 - 2026-05-21

- Removed hidden in-memory fallback behavior from remote-backed CLI commands so missing remote configuration now fails clearly.
- Kept explicit projection workflows local, including dry-run push command previews from supplied projections.
- Improved replay and human-output test coverage for remote command behavior without test-only runtime overrides.

## 0.0.4 - 2026-05-19

- Added `poly self-update` for shell-installed ADK releases.
- Improved status snapshots for function metadata and key resource types.
- Fixed formatting baselines and YAML/resource ordering parity.

## 0.0.3 - 2026-05-18

- Improved Python parity for prompt references by materializing readable names in `rules`, `topics`, and flow steps, while preserving push round-tripping back to Agent Studio IDs.
- Aligned flow function import path behavior with Python by materializing `flows.<flow_name>.functions` imports and translating back to ID-based paths during push.
- Improved interactive UX parity by filtering deleted projects from selection, adding arrow-key/fuzzy project selection, and restoring terminal state with a clean "Cancelled by user" Ctrl-C path.
- Tightened generated resource parity with Python by matching file headers/formatting details and expanding recording/replay coverage for interactive and flow import-path contracts.

## 0.0.2 - 2026-05-15

- Improved CLI help parity and release workflow caching.
- Fixed push/pull behavior for larger projects with complex function files.
- Improved status hashing for Python-compatible local project snapshots.

## 0.0.1 - 2026-05-15

- Initial cargo-dist release setup for the Rust ADK CLI.
