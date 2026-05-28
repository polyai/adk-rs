# adk-rs Agent Instructions

## Collaboration preferences

- Prioritize Python ADK parity over placeholder compatibility.
- Always replicate the Python ADK's behavior, even when it looks quirky.
- If the Python behavior appears buggy, add a comment at the offending Python code before
  implementing a Rust-side workaround.
- Avoid dummy/no-op implementations in CLI/core/platform paths.
- When a behavior is not truly implemented, track it explicitly as a TODO.
- Keep the PR-ready summary as the final step, after implementation/test work.
- Never amend or force-push branches unless the user explicitly requests a
  history rewrite.

## Implementation priorities

- Typed resource discovery/lifecycle parity is core and should remain a primary design anchor.
- Prefer trait-based and metadata-driven mappings that mirror Python naming/structure.
- For branch/status/diff/push/pull flows, verify whether behavior is local-only vs server-backed.
- Do not silently substitute in-memory behavior for real remote semantics in user-facing flows.
- Put resource-family-specific semantics in `adk-resources`: discovery facts, local file layout,
  projection materialization, validation helpers, typed lifecycle helpers, stable IDs, and command
  generation helpers should stay near modules named for the resource family.
- In `adk-resources`, reserve top-level directories for ADK resource families. Cross-cutting
  orchestration and shared helpers should live in top-level Rust modules/files unless they are nested
  inside a resource-family module.
- Use the local file layout taxonomy from `docs/development.md` (`singletons`, `aggregates`, and
  `per_resource_files`) as a resource property or migration aid, not as the long-term module boundary.
  Avoid older vague buckets such as `single-file` or `structured` for new or refactored modules.
- Filesystem access in library crates should go through `adk-io`: do not add new direct `std::fs`
  usage in `adk-core` or other reusable library logic, and prefer generic `Fs: FileSystem` APIs so
  each binary/test/FFI target chooses `StdFileSystem` or `MemoryFileSystem` at compile time. Direct
  `std::fs` usage is acceptable in `adk-cli` and test harnesses; avoid `dyn FileSystem` unless
  runtime filesystem selection is specifically needed.
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
- We are not publishing a public Rust API yet, so breaking compatibility of pub functions
  is not a problem. Our only compatibility obligations are to the on-disk file structure that
  ADK materializes, and the remote server's HTTP API.

## Release notes

- When cutting a release, update `CHANGELOG.md` before bumping versions/tagging.
- Keep `CHANGELOG.md` concise, user-facing, and free of implementation archaeology.
- Cover only the 3-5 highest-signal user-visible behavior changes and parity fixes.
- Move relevant `Unreleased` bullets into `## <version> - <YYYY-MM-DD>`, then reset `Unreleased`.

## Codex Cloud limitations

- In Codex Cloud mode, Codex cannot push commits or tags to this repository. If asked to perform a
  task that requires those permissions, such as cutting a release, respond that Cloud-mode Codex is
  unable to do it.

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

## Pull Requests

- Pull request titles and commit messages should follow the Conventional Commits spec.
- When asked to babysit a PR through review and merge, immediately mark it ready and wait for Codex review.
- You'll know Codex has begun a review when it adds an eyes emoji to the summary. You'll know
  Codex approves the PR when it adds a thumbs-up emoji on the summary. If it takes more than
  5 minutes with no comments or approval, ask the user for human review.
