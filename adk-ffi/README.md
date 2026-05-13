# poly-adk-ffi

FFI-oriented wrapper crate for embedding ADK from other runtimes.

## Responsibilities

- Exposes stable, serialization-friendly entrypoints over `poly-adk-core`.
- Keeps interop-facing payloads simple (JSON and plain structs).

This crate is the bridge point for future Python and TypeScript native extensions.
