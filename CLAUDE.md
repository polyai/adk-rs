# adk-rs Agent Memory

## Collaboration preferences

- Prioritize Python ADK parity over placeholder compatibility.
- Avoid dummy/no-op implementations in CLI/core/platform paths.
- When a behavior is not truly implemented, track it explicitly as a TODO.
- Keep the PR-ready summary as the final step, after implementation/test work.

## Implementation priorities

- Typed resource discovery/lifecycle parity is core and should remain a primary design anchor.
- Prefer trait-based and metadata-driven mappings that mirror Python naming/structure.
- For branch/status/diff/push/pull flows, verify whether behavior is local-only vs server-backed.
- Do not silently substitute in-memory behavior for real remote semantics in user-facing flows.

## Quality expectations

- Add verification tests with each substantive parity change.
- Expand unit coverage in sub-crates when gaps are identified.
- Treat audit findings as actionable backlog unless blocked by missing API/schema contracts.
