# @poly-ai/adk-node

N-API wrapper for pure PolyAI ADK workflows.

This crate is a thin synchronous adapter over `adk-core`. It accepts
TypeScript-friendly file maps and projection JSON strings, loads them into an
in-memory filesystem, and returns pull snapshots or push command batch bytes.

The colocated npm package exposes the public TypeScript API. It accepts
projection objects, serializes them before crossing the native boundary, and
normalizes native failures into `AdkNapiError`.

```bash
npm install
npm test
```

The native binding is built by NAPI-RS as part of `npm test`. The generated
`.node` binary and TypeScript build output are intentionally ignored by Git.

To smoke test the shared wrapper tests against the package currently published
to npm, run:

```bash
npm run test:published -- @poly-ai/adk-node@rc
```
