# TypeScript N-API ADK Wrapper Plan

## Motivation

The Rust ADK already contains the parity-sensitive logic for materializing
Agent Studio projections, discovering local resources, validating project
files, and generating push commands. We want to expose that logic to
TypeScript for backend and Node-like environments without rewriting ADK
semantics in TypeScript.

The proposed TypeScript library should be a pure project/projection library:
all operations are functions of caller-provided inputs. TypeScript owns
transport concerns such as auth, endpoint selection, fetch, retries, and
posting command batches. Rust owns the ADK-specific interpretation of files
and projections.

N-API is a good fit when the target is backend JavaScript: it avoids the
WebAssembly boundary and packaging constraints while still providing a stable
Node native-addon interface.

## Core Principles

- No HTTP or backend calls in the TypeScript wrapper package.
- No `adk-api-client` dependency in the TypeScript-facing wrapper crate graph.
- No local disk access for the TypeScript wrapper APIs.
- No environment variable reads.
- No subprocesses such as `ruff`.
- No `_gen/.agent_studio_config` local status file.
- Inputs and outputs use TypeScript-friendly data shapes.
- Projection inputs are passed as JSON strings, serialized by the TypeScript
  wrapper before entering Rust.
- Push command batch bytes are produced by Rust and posted by TypeScript.

## Desired Crate Structure

The implementation should split pure core workflow behavior from CLI and
transport behavior.

```text
adk-core
  Pure project/projection workflows.
  Depends on adk-io, adk-resources, adk-protobuf, adk-types.
  Does not depend on adk-api-client.

adk-napi
  NAPI-RS TypeScript wrapper over adk-core.
  Accepts JSON strings and Record<string, string>-style file maps.
  Returns JS-friendly result objects and Uint8Array command batch bytes.

adk-api-client
  HTTP transport only.
  Fetches projections and posts command batches for native CLI/service flows.

adk-service
  Orchestration glue for the CLI/native library.
  Combines adk-core behavior with adk-api-client transport.

adk-cli
  Command-line argument parsing, local disk filesystem use, credentials,
  user output, and existing CLI workflows.
```

This intentionally reuses the `adk-core` name for the pure crate. The current
API-aware orchestration responsibilities in `adk-core` should move into
`adk-service`.

The important boundary is that `adk-core` must be usable by `adk-napi`
without pulling in `adk-api-client` or transport behavior. Unlike the Wasm
plan, `adk-core` does not need to be constrained to `wasm32`-compatible
dependencies, but it should still avoid hidden native side effects in its pure
APIs.

## TypeScript API Surface

The v1 library should expose two operations: `pull` and `push`.

### File Map

All operations use a text-only in-memory filesystem:

```ts
type FileMap = Record<string, string>;
```

`FileMap` is a TypeScript and `adk-napi` boundary type. It should not become
an `adk-core` abstraction. The `adk-napi` crate is responsible for loading this
object into a concrete `MemoryFileSystem` before calling filesystem-generic
core logic, and for exporting the resulting memory filesystem back to a
`FileMap` when needed.

File map keys are normalized POSIX-style relative paths:

- No leading slash.
- `/` path separators.
- UTF-8 text content.
- ADK resource files are interpreted by the Rust ADK logic.
- Unrelated files are preserved unless explicitly overwritten or deleted by
  ADK-owned pull behavior.

Each operation also receives an explicit `root` string from the caller. File map
keys are interpreted relative to that root after they are loaded into
`MemoryFileSystem` by `adk-napi`; `adk-core` should not discover or invent a
root path for the TypeScript bindings.

If `_gen/.agent_studio_config` is present in the input, the wrapper ignores it
and never emits it. Other deterministic helper files under `_gen/**` may be
generated or refreshed by pull, matching native CLI project materialization.

### Pull

```ts
type PullInput = {
  root: string;
  files: FileMap;
  pullProjection: unknown;
  baseProjection?: unknown;
  force?: boolean;
};

type FileChange =
  | { path: string; kind: "write"; content: string }
  | { path: string; kind: "delete" };

type PullOutput = {
  files: FileMap;
  changes: FileChange[];
  conflicts: string[];
};
```

`pullProjection` is the projection to materialize into files. The handwritten
TypeScript wrapper serializes it to JSON before calling the raw native binding.

`baseProjection`, when provided, describes the projection that the current files
were originally materialized from. This enables conflict detection without
storing `_gen/.agent_studio_config`.

Pull conflict behavior:

- With `baseProjection`, the Rust core can compare base materialized files,
  caller-provided files, and pull-projection materialized files.
- If a file is unchanged from base locally, the pull projection content may be
  written.
- If both local and pull projection changed relative to base and contents
  differ, the path is reported in `conflicts`.
- If `force` is true, pull projection materialization wins for ADK-owned files.
- Without `baseProjection`, the Rust core cannot know whether existing local
  content is user-edited or previously materialized. TypeScript documentation
  should describe this as a conservative mode: callers should pass `force` to
  overwrite, or pass `baseProjection` for conflict-aware pulls.

`files` returns the full updated snapshot. `changes` returns the corresponding
patch for editor and UI integrations.

### Push

```ts
type PushInput = {
  root: string;
  files: FileMap;
  projection: unknown;
  lastKnownSequence: number;
  createdBy?: string;
  currentTime?: string | Date;
  force?: boolean;
  skipValidation?: boolean;
};

type PushOutput = {
  success: boolean;
  message?: string;
  commandBatchBytes?: Uint8Array;
};
```

`projection` is the inner Agent Studio projection object, not the full backend
projection response. The handwritten TypeScript wrapper serializes it to JSON
before calling the raw native binding. `lastKnownSequence` is passed separately
and is encoded into the returned protobuf command batch.

Rust should build the protobuf `CommandBatch` bytes. TypeScript should post
those bytes to the backend with the correct transport details.

`currentTime` controls command metadata timestamps and should be an ISO
8601/RFC 3339 timestamp string. The TypeScript wrapper may default this to the
current time before calling Rust. The Rust core workflow should not read the
system clock for this value when it is supplied.

Command IDs are generated internally as UUID v4 values. They are intentionally
not deterministic in v1. As a result, `commandBatchBytes` are not byte-stable
across identical calls. Tests that need deterministic assertions should decode
command batches and ignore command IDs.

`message` is optional and should be set only when push cannot produce a command
batch.

`skipValidation` is retained for CLI parity and advanced callers. Formatting is
not part of the TypeScript wrapper because it currently depends on subprocess
behavior in native CLI flows.

For v1, push should generate commands from the full caller-provided file map
against the supplied projection. Changed-resource optimization can be added
later if the caller provides an explicit baseline.

## Projection Handling

The TypeScript wrapper should serialize projection objects before calling into
the native addon:

```ts
const projectionJson = JSON.stringify(projection);
```

All projection JSON inputs are the inner Agent Studio projection object, not
the full backend response wrapper.

The N-API layer should parse that string into Rust-owned JSON data, then call
the existing projection materialization and command-generation logic.

The projection is currently treated as a dynamic `serde_json::Value` tree in
Rust. Resource-family-specific modules know how to read backend shapes such as
`{ ids, entities }`, tolerate missing fields, apply Python-compatible defaults,
and materialize typed local resources or protobuf commands. The v1 N-API
design should preserve that schema-informed but not fully schema-typed
approach.

## Error Contract

N-API-facing errors should use stable TypeScript-friendly codes rather than raw
Rust error strings alone.

```ts
type AdkNapiError = {
  code:
    | "INVALID_INPUT"
    | "INVALID_PROJECTION"
    | "VALIDATION_FAILED"
    | "CONFLICT"
    | "COMMAND_GENERATION_FAILED"
    | "INTERNAL_ERROR";
  message: string;
  details?: unknown;
};
```

The wrapper may throw these errors or return result objects with an error
field, but the shape should be stable.

## File Preservation Rules

- Unknown, non-ADK files in `files` are preserved in output snapshots.
- Pull may write ADK-owned files materialized from the pull projection.
- Pull may generate or refresh deterministic Python helper files under
  `_gen/**`.
- Pull with `force` may delete ADK-owned local-only files that are not present
  in the pull projection.
- Pull without `force` should avoid destructive changes when conflicts are
  detected.
- `_gen/.agent_studio_config` is ignored if present and never emitted.
- Other `_gen/**` helper files are deterministic generated project support
  files. Pull may create or update them, while push and resource collection
  ignore them as non-resource files.

## Determinism Contract

The core workflow should be deterministic for:

- Projection materialization.
- Conflict detection.
- Validation results.
- Logical command selection and ordering.

Push command metadata timestamps are deterministic when `currentTime` is
provided. The TypeScript wrapper should make `currentTime` optional for
ergonomics, but should pass an explicit value into Rust.

Push command IDs are UUID v4 values generated inside Rust and are not
deterministic in v1. Therefore encoded command batch bytes are not guaranteed
to be byte-stable across identical calls.

## Implementation Game Plan

The work should be split into small reviewable phases. The architectural
target is:

- `adk-core` becomes the pure file/projection crate.
- `adk-core` depends on `adk-io`, `adk-resources`, `adk-protobuf`, and
  `adk-types`.
- `adk-api-client` is removed from the `adk-core` dependency graph.
- API-aware orchestration moves from the current `adk-core` shape into
  `adk-service`.
- `adk-cli` calls `adk-service` so existing native CLI behavior is preserved.
- Resource-family-specific behavior stays in `adk-resources`.

The phases below are a concrete path toward that target. They can be landed as
small boundary-preserving slices rather than strictly serial phases. In
particular, once an initial pure push-planning slice exists, it is useful to
establish the `adk-service` crate boundary early so later pull extraction
happens with the dependency direction already visible. The N-API boundary
should still wait until the core behavior it needs is stable.

### Phase 1: Filesystem-Backed Core Workflow APIs

Refactor the existing Rust workflow code so the reusable pieces in `adk-core`
continue to operate through the `adk_io::FileSystem` trait and parsed
projection values. This should be an extraction of the current
filesystem-backed discovery, materialization, validation, pull, and
command-generation paths needed by the TypeScript wrapper, not a new parallel
implementation. Existing native CLI diff behavior is not part of the v1
TypeScript wrapper surface and can remain in the service/native workflow path.

Projection JSON parsing should not live deep in reusable workflow code. The
filesystem-backed `adk-core` helpers should accept parsed
`serde_json::Value` projections and leave string parsing to their callers.

Suggested `adk-core` internal Rust shapes, not direct N-API exports and not
boundary wrappers:

```rust
pub struct PullInput {
    pub pull_projection: serde_json::Value,
    pub base_projection: Option<serde_json::Value>,
    pub force: bool,
}

pub struct PushInput {
    pub projection: serde_json::Value,
    pub last_known_sequence: u64,
    pub created_by: Option<String>,
    pub current_time: Option<chrono::DateTime<chrono::Utc>>,
    pub force: bool,
    pub skip_validation: bool,
}

pub fn pull<Fs: adk_io::FileSystem>(
    fs: &Fs,
    root: &std::path::Path,
    input: PullInput,
) -> Result<PullOutput, CoreError>;

pub fn push<Fs: adk_io::FileSystem>(
    fs: &Fs,
    root: &std::path::Path,
    input: PushInput,
) -> Result<PushOutput, CoreError>;
```

Do not add a new `adk-core` facade whose main job is to translate external
caller inputs or to reimplement existing service behavior. The intended
migration is to move/refine current logic into pure helper functions with
cleaner signatures, then have both `adk-service` and `adk-napi` call those
helpers.

The core implementation should remain generic over the `adk_io::FileSystem`
trait where filesystem behavior is needed. `MemoryFileSystem` is the concrete
adapter used by tests, not the abstraction that core logic should depend on
directly. The goal is to preserve the existing filesystem-backed code path as
much as possible rather than introduce a second direct snapshot
implementation.

This phase should also remove or isolate remaining native filesystem
assumptions in pure workflows:

- Pure `adk-core` APIs should take an explicit root and should not define a
  global root-path convention. Native project root discovery is a CLI/service
  concern; embedded callers that bypass CLI discovery must provide the root. If
  any reusable discovery helper remains in `adk-core`, it should use an
  injected `adk_io::FileSystem`, not `StdFileSystem`.
- Force-pull deletion cleanup should work through `adk_io::FileSystem`, not
  `WalkDir` over disk.
- Status file persistence should move out of pure workflows. `adk-core` may
  keep value-level helpers for computing status-like snapshots, hashes, or
  conflict baselines, but it should not read or write
  `_gen/.agent_studio_config` for these APIs.

Verification for this phase:

- Compile-time checks should prove `adk-core` no longer depends on
  `adk-api-client`.
- Unit tests should cover the extracted filesystem-generic helpers directly,
  using `MemoryFileSystem` in tests where useful but without making
  `MemoryFileSystem` the core API abstraction.
- Existing filesystem-generic tests should pass with both `StdFileSystem`
  where appropriate and `MemoryFileSystem`.

### Phase 2: Pull and Push Semantics

Implement the two agreed TypeScript wrapper operations in `adk-core`.

Pull:

- Materialize `pull_projection` into ADK-owned files.
- If `base_projection` is provided, materialize it too and use it as the
  conflict baseline.
- Preserve unrelated files from the caller-provided filesystem snapshot.
- Return both the full output snapshot and a patch list.
- Generate or refresh deterministic Python helper files under `_gen/**`.
- Never emit `_gen/.agent_studio_config`.

Push:

- Collect local resources from the input files.
- Validate unless `skip_validation` is true.
- Build push commands against the supplied projection.
- Encode an `adk_protobuf::CommandBatch` with `last_known_sequence`.
- Return command batch bytes.
- Use the caller-provided `current_time` for command metadata when supplied.
- Continue using UUID v4 command IDs in v1.

Verification for this phase:

- Pull tests should cover conflict behavior with and without
  `base_projection`, force overwrite behavior, patch output, and preservation
  of unrelated files.
- Push tests should decode protobuf bytes to verify `last_known_sequence` and
  command ordering while ignoring UUID values.
- Projection-to-files parity tests should reuse existing Python ADK fixtures
  where possible.

### Phase 3: Service Boundary

Add `adk-service` as the API-aware orchestration crate.

- Own the `PlatformClient` trait boundary or depend on the one exposed by
  `adk-api-client`.
- Move API-aware orchestration from the current `adk-core` shape into
  `adk-service`.
- Combine `adk-core` operations with `adk-api-client` transport.
- Own native CLI persistence of `_gen/.agent_studio_config`, including when to
  read it as a baseline and when to write it after successful pull/push flows.
  During the transition, `adk-service` may call status helper methods that
  still live in `adk-core`; pure core workflow APIs should not expose those side
  effects.
- Keep branch/deployment/chat operations that require remote state out of
  `adk-core`.
- Have `adk-cli` call `adk-service`.
- Keep CLI-facing semantics, output messages, and existing tests passing.

This lets the native CLI keep its current behavior while the TypeScript wrapper
uses the same pure core without transport.

Verification for this phase:

- Existing CLI and service workflow tests should continue to pass.
- Tests should prove `_gen/.agent_studio_config` is read/written by
  `adk-service` for native pull/push flows, not by pure `adk-core` APIs.
- Dependency checks should confirm `adk-service` may depend on
  `adk-api-client`, while `adk-core` does not.

### Phase 4: N-API Wrapper

Add `adk-napi` once the pure Rust APIs are stable.

- Expose `pull` and `push` through NAPI-RS.
- Accept projection inputs as JSON strings at the raw native boundary.
- Accept and return file maps as TypeScript-friendly objects.
- Convert errors into stable `AdkNapiError` values.
- Return command batch bytes as `Uint8Array`.
- Provide handwritten TypeScript wrapper code that handles `JSON.stringify`,
  defaulting `currentTime`, and passing the explicit root through to the raw
  native binding.

Suggested `adk-napi` exported Rust functions should stay data-oriented and
simple. The raw native-addon boundary can take ordinary NAPI-RS-compatible
Rust/Serde shapes, while the handwritten TypeScript wrapper keeps the
ergonomic object-shaped API described above.

```rust
#[napi(object)]
pub struct NapiPullInput {
    pub root: String,
    pub files: std::collections::BTreeMap<String, String>,
    pub pull_projection_json: String,
    pub base_projection_json: Option<String>,
    pub force: Option<bool>,
}

#[napi(object)]
pub struct NapiPushInput {
    pub root: String,
    pub files: std::collections::BTreeMap<String, String>,
    pub projection_json: String,
    pub last_known_sequence: u64,
    pub created_by: Option<String>,
    pub current_time: Option<String>,
    pub force: Option<bool>,
    pub skip_validation: Option<bool>,
}

#[napi]
pub fn pull(input: NapiPullInput) -> napi::Result<NapiPullOutput>;

#[napi]
pub fn push(input: NapiPushInput) -> napi::Result<NapiPushOutput>;
```

These exports should be synchronous. They perform CPU and in-memory filesystem
work only; TypeScript remains responsible for async network transport.

For `commandBatchBytes`, `adk-napi` should return a `Uint8Array` generated
from the Rust protobuf bytes.

These exported functions are responsible for boundary conversion only: JSON
strings to parsed projections, file maps to `MemoryFileSystem`, and
filesystem-backed results back to TypeScript-friendly objects.

The N-API boundary can use richer native-addon conversions than Wasm, but it
should still remain a thin, synchronous adapter over `adk-core`.

Verification for this phase:

- N-API-facing tests should cover TypeScript wrapper shapes and raw native
  boundary conversion.
- N-API tests should cover `FileMap` to `MemoryFileSystem` loading/export, path
  normalization, and `_gen/.agent_studio_config` exclusion.
- Tests should cover bad projection JSON and stable `AdkNapiError`
  conversion.
- Push tests should verify `commandBatchBytes` is returned as `Uint8Array`.
- Packaging smoke tests should load the native addon in the supported backend
  Node versions.

### Cross-Phase Test Strategy

Testing should happen throughout the migration, not only at the end. Each
phase should land with tests that enforce its new boundary or behavior before
later phases build on it.

The main test contracts are:

- `adk-core` stays transport-free and does not depend on `adk-api-client`.
- Pure workflows use injected filesystem behavior and do not read or write
  `_gen/.agent_studio_config`.
- Native CLI workflows keep their existing status-file behavior through
  `adk-service`.
- Projection-to-files materialization stays compatible with Python ADK
  fixtures.
- Push command selection and ordering stay stable at the logical summary level.
- Encoded command batches include the supplied `last_known_sequence`, while
  UUID command IDs remain nondeterministic.
- Pull conflict behavior is covered with and without `base_projection`.
- N-API wrapper behavior is covered at the TypeScript-facing boundary.

The migration is complete when existing CLI tests still pass and the extracted
filesystem-backed `adk-core` APIs used by `adk-napi` can run without
`adk-api-client`, network access, disk access, env vars, subprocesses, or
`_gen/.agent_studio_config`.

## Open Follow-Up Work

- Add the platform-specific prebuild/publish workflow for the colocated
  `adk-napi` npm package.
- Revisit the final npm package name and publish visibility before release.
- Keep Node.js support at `>=20` for the first wrapper package; the Rust N-API
  binding currently targets Node-API 4.
- Consider a future Wasm package if browser or edge-runtime portability becomes
  a real target.
- Consider a future changed-resource push mode with an explicit baseline
  projection or baseline file map.
