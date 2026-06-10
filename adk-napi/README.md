# poly-adk-napi

N-API bindings for the pure PolyAI ADK engine.

This crate is a thin synchronous adapter over `adk-core`. It accepts
TypeScript-friendly file maps and projection JSON strings, loads them into an
in-memory filesystem, and returns pull snapshots or push command batch bytes.
