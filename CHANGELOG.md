# Changelog

Concise user-facing release notes for the Rust ADK CLI.

## Unreleased

- _Nothing yet._

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
