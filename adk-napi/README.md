# poly-adk-napi

N-API wrapper for pure PolyAI ADK workflows.

This crate is a thin synchronous adapter over `adk-core`. It accepts
TypeScript-friendly file maps and projection JSON strings, loads them into an
in-memory filesystem, and returns pull snapshots or push command batch bytes.
