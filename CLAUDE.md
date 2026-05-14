# adk-rs Agent Memory

## Collaboration preferences

- Prioritize Python ADK parity over placeholder compatibility.
- Always replicate the Python ADK's behavior, even when it looks quirky.
- If the Python behavior appears buggy, add a comment at the offending Python code before
  implementing a Rust-side workaround.
- Avoid dummy/no-op implementations in CLI/core/platform paths.
- When a behavior is not truly implemented, track it explicitly as a TODO.
- Keep the PR-ready summary as the final step, after implementation/test work.

## Implementation priorities

- Typed resource discovery/lifecycle parity is core and should remain a primary design anchor.
- Prefer trait-based and metadata-driven mappings that mirror Python naming/structure.
- For branch/status/diff/push/pull flows, verify whether behavior is local-only vs server-backed.
- Do not silently substitute in-memory behavior for real remote semantics in user-facing flows.
- Filesystem access in library crates should go through `adk-io`. Do not add new direct
  `std::fs` usage in `adk-core` or other reusable library logic; migrate existing call sites to
  `FileSystem`/`StdFileSystem`/`MemoryFileSystem`. Direct `std::fs` usage is acceptable in
  `adk-cli` and test harnesses. Once remaining library call sites are gone, enforce this
  mechanically with Clippy `disallowed-methods`.

## Quality expectations

- Add verification tests with each substantive parity change.
- Expand unit coverage in sub-crates when gaps are identified.
- Treat audit findings as actionable backlog unless blocked by missing API/schema contracts.
