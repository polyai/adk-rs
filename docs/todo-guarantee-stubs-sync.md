# TODO: Guarantee Runtime Stub Sync

## Context

The upstream Python ADK recently added a script to regenerate `src/poly/types`
from `genai_lambda_runtime/python/runtime`, then fixed the generated import style
and added integration stubs. Rust ADK also vendors generated Python helper files
under `adk-core/python-gen-template` so `poly init` and `poly pull` can populate
project `_gen/` packages for function autocomplete.

Because this Rust project is intended to replace the Python ADK, we should not
depend on the Python ADK repository as an input. The canonical source should be
the runtime package API surface, with generated templates checked into this repo.

## Upstream Follow-Up

After the initial Python ADK generator landed in #184 and the relative-import
fix landed in #187, upstream merged #188 to fix another `_gen` sync bug:
`save_imports` still copied only top-level `poly.types/*.py` files, so the new
`integrations/` package was omitted from generated project `_gen/` folders.
Upstream then moved the generated type surface to `.pyi` files in #189, using
mypy `stubgen` and `imports.json` as the public source manifest.

The lesson for Rust ADK is that recursive package handling and `.pyi` handling
should be part of the initial design, not later cleanup. A sync implementation
must recursively generate, copy, embed, and write nested type stub files, while
preserving the small real `.py` files that define the `_gen` import surface.

## Near-Term Baseline

First pass: `scripts/sync_runtime_gen_templates.py` can refresh
`adk-core/python-gen-template` directly from
`genai_lambda_runtime/python/runtime`.

Expected shape:

- Input: path to `genai_lambda_runtime/python/runtime`.
- Output: checked-in `.pyi` templates under `adk-core/python-gen-template`.
- Preserves local ADK helper shims such as `decorators.py`.
- Generates `_gen/__init__.py` from exported names in generated stubs.
- Generates support-only `.pyi` files for sibling runtime imports without
  re-exporting those names from `_gen`.
- Copies nested packages recursively so APIs such as integrations work.
- Provides `--check` mode that regenerates into a temporary directory and fails
  when checked-in templates differ.
- Documents the refresh command in `docs/development.md`.

This would copy the upstream Python ADK approach closely enough to solve the
immediate maintenance pain without creating a dependency on the Python ADK repo.

Remaining near-term work:

- Decide whether `--sync-fixture` should become part of the documented normal
  refresh workflow or remain an explicit maintainer option.

Real-runtime smoke status:

- Generated templates from `/home/ben/genai_lambda_runtime/python/runtime` into
  a temporary `_gen` package.
- Switched the generated runtime surface to `.pyi`, matching upstream #189.
- Taught Rust embedding/writing paths to handle recursive template files.
- Updated checked-in templates and the fixture `_gen` tree from the runtime.

## Stronger Drift-Reduction Idea

When we have time, make drift mechanically harder by deriving more from the
runtime tree and less from hand-maintained generator configuration.

Preferred design:

- Walk `genai_lambda_runtime/python/runtime/**/*.py` instead of maintaining a
  fixed `STUB_FILES` list.
- Select public modules by clear inclusion rules and a small denylist for truly
  internal implementation modules.
- Use AST transformation where possible: preserve module/class/function syntax,
  delete private definitions, replace executable bodies with `...`, and keep
  public signatures, properties, type aliases, imports, and docstrings.
- Generate each module's `__all__` from the transformed public surface when the
  source module does not already define one.
- Generate `_gen/__init__.py` from every generated module's `__all__`, including
  nested modules.
- Make the generated template directory the single source of truth for Rust
  packaging by recursively embedding `python-gen-template/**/*.py` and
  `python-gen-template/**/*.pyi` at build time, instead of maintaining a
  hardcoded flat file list.

## Contract Snapshot

Consider inserting an intermediate manifest:

```text
runtime source -> public API manifest JSON -> _gen templates
```

The manifest would describe module names, exported symbols, classes, methods,
properties, signatures, type aliases, and relevant imports. This gives reviews a
compact way to see that the runtime public API changed, separately from generated
Python formatting churn.

## Long-Term Ideal

The least-drifty solution would be for `genai_lambda_runtime` itself to publish a
stub or public API artifact as part of its release process. Rust ADK could then
consume that artifact directly while still checking the resulting templates into
this repository for end-user installs.

Until that exists, a Rust-owned sync script plus `--check` mode is the pragmatic
path.
