# adk-rs Agent Instructions

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
- Preserve clean Git diffs for ADK-maintained project files. Users check these files into Git and
  rely on `pull`/`push` to inspect meaningful backend sync changes, so semantically irrelevant YAML
  rewrites, key reordering, scalar restyling, and formatting churn should be minimized.

## Quality expectations

- Add verification tests with each substantive parity change.
- Expand unit coverage in sub-crates when gaps are identified.
- Treat audit findings as actionable backlog unless blocked by missing API/schema contracts.
- Add Rust doc comments for public or crate-public functions when they are long, multi-parameter,
  or orchestrate non-obvious behavior. Prefer information-dense docs that explain modes, side
  effects, ordering constraints, and Python parity assumptions rather than restating the signature.

## Release notes

- When cutting a release, update `CHANGELOG.md` before bumping versions/tagging.
- Keep `CHANGELOG.md` concise, user-facing, and free of implementation archaeology.
- Cover only the 3-5 highest-signal user-visible behavior changes and parity fixes.
- Move relevant `Unreleased` bullets into `## <version> - <YYYY-MM-DD>`, then reset `Unreleased`.

## Review guidelines

- Focus first on behavior regressions against the Python ADK, especially CLI contracts, JSON output
  shape, project file layout, command generation, and status/diff/push/pull semantics.
- Treat missing or weak tests as important when a PR changes user-visible behavior, parity-sensitive
  formatting, command payloads, release workflow, or file lifecycle logic.
- Flag changes that introduce placeholder behavior, in-memory fallback in user-facing flows,
  unnecessary YAML/key-order churn, or direct `std::fs` use in reusable library logic.
- Scrutinize release changes, installer/self-update behavior, generated artifacts, auth/API client
  changes, and broad refactors for unintended blast radius.
- Prefer concise, actionable findings. Do not block on style-only issues unless they obscure behavior
  or violate established project conventions.
