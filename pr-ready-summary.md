## PR Title
Improve Python parity for named-state diffing, command ordering, and remote command semantics

## Summary
- Implemented Python-compatible named-state resolution for `diff --before/--after`, including environment names, branch names/IDs, deployment hash prefixes, and the `after-only => previous version` rule.
- Removed multiple parity placeholders across CLI/core/platform layers: real revert behavior, pull/push flag semantics, explicit fallback guardrails, symmetric diff handling, richer projection mapping, and richer push command payloads.
- Aligned push command queue ordering with Python-style priority behavior for supported resource families and added tests to guard ordering regressions.

## Commit-by-Commit Rationale (Suggested Split)
1. **CLI/core behavior parity and safety**
   - Real `revert` implementation via core service and CLI wiring.
   - `pull --force` conflict overwrite semantics and `push` flag behavior (`skip_validation`, `dry_run`).
   - Explicit remote-client/fallback guardrails for mutating and remote-backed CLI commands.
2. **Status/diff contract alignment**
   - Added explicit `conflict_detection_available` in status outputs (CLI + FFI + domain).
   - Made `adk-io::diff_resources` symmetric for before-only deletions.
3. **Platform API response/projection parity**
   - Parse command-batch push responses instead of discarding returned data.
   - Improve pull projection mapping for variables, entities, handoff SIP config/headers.
   - Use authoritative IDs when available and reduce synthetic fallback IDs.
4. **Metadata/reference parity**
   - Replace placeholder actor identity semantics with validated identity resolution.
   - Expand SMS and stop-keyword reference mapping to include real maps from local/projection data.
   - Enrich function create/update payload metadata (description/parameters/errors/archived).
5. **Named-state diff parity**
   - Extend platform client API with `pull_resources_by_name`.
   - Implement HTTP named-state lookup matrix (env/branch/version hash).
   - Implement core `after-only` previous-version resolution with Python-compatible error paths.
6. **Ordering parity hardening**
   - Apply explicit command-type priority ordering across delete/create/update phases for supported families.
   - Add regression tests for variable-first priority behavior across phases.

## Reviewer Notes
- `adk-platform-api/src/lib.rs` now carries most parity logic: named-state remote resolution, projection conversion, and cross-family command queue ordering.
- `adk-platform-api/src/push_extended.rs` now includes richer reference/config mapping; this file has many parity-sensitive command builders and deserves focused review.
- `adk-core/src/lib.rs` diff flow changed significantly for named-state and `after-only` behavior; compare against Python `project.py` logic for expected edge-case outcomes.
- `adk-cli/src/main.rs` fallback semantics changed intentionally: silent in-memory fallback for remote-backed commands is now opt-in via `POLY_ADK_ALLOW_INMEMORY_FALLBACK=1`.

## Test Plan
- Formatting:
  - `cargo fmt`
- Core parity and behavior:
  - `cargo test -p adk-core`
- Platform command/projection parity:
  - `cargo test -p adk-platform-api`
- CLI contract and surface:
  - `cargo test -p adk-cli`
- FFI status contract:
  - `cargo test -p adk-ffi`
- Supporting crates:
  - `cargo test -p adk-io`
  - `cargo test -p adk-domain`

## Risk Areas
- Named-state remote lookup depends on deployment endpoint payload shape differences across environments.
- Priority ordering is now explicit by command type strings; adding new command families requires updating priority tables/tests.
- Function metadata inference from local code is best-effort; remote metadata remains authoritative when present.
