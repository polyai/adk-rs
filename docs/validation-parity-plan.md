# Validation Parity Plan

This plan captures the structural fix for DEVP-319: Rust ADK validation must
match Python ADK validation as a resource contract, not as a loose set of
best-effort file checks.

## Problem

Python ADK validation reads every discovered local resource into its typed
resource class, runs `resource.validate(resource_mappings=...)`, then runs
`resource_type.validate_collection(...)` for the resource family. That makes
validation part of the authoring surface: local project files are expected to
obey the same semantic rules as Agent Studio resources.

Rust ADK validation currently parses local YAML/JSON, calls optional
resource-local raw YAML checks, then runs a handful of bespoke cross-resource
passes. The `DiscoverResources::validate_local_yaml` hook has a no-op default,
so adding discovery for a resource family does not require adding the Python
validator contract. This made missing validation the default outcome and led to
many partial, symptom-driven ports.

## Goals

- Make Python ADK validation parity explicit for every resource family.
- Preserve Python behavior, including quirky error timing and collection-level
  checks, unless a reviewer accepts a documented divergence.
- Keep each resource family's business rules close to that resource's
  implementation.
- Use typed local resource views for validation where possible, rather than
  ad hoc YAML traversal.
- Keep resource-family rules in `adk-resources` and cross-resource orchestration
  in `adk-core/src/validation.rs`.
- Add parity tests with every substantive validation change.

## Non-Goals

- Do not redesign the public Rust API around validation yet.
- Do not depend on live Agent Studio calls for ordinary validation tests.
- Do not silently replace Python behavior with stricter or cleaner Rust-only
  behavior without documenting and reviewing the divergence.

## Target Architecture

### 1. Resource-Local Validation Contract

Validation rules should live beside the resource-family implementation in
`adk-resources`. Handoff rules belong in `handoffs`, API integration rules in
`api_integrations`, flow-local rules in `flows`, and so on. The point is to make
validation part of each resource family's semantics, not a distant central
policy table.

For tracking, prefer inline validation parity markers on each resource type or
resource-family module instead of a separate registry. At the beginning of the
migration, every resource should get an easy-to-search marker such as:

```rust
// Validation parity: TODO(DEVP-319) audit Python SettingsRole.validate().
```

As each family is migrated, replace the marker with an explicit status:

```rust
// Validation parity: implemented against Python SettingsRole.validate().
```

Each Python resource type should have one of these statuses directly next to
the resource implementation:

- `implemented`: Rust has a validator intended to match Python.
- `not_applicable`: Python has no meaningful validator for this resource type.
- `todo`: parity is known missing and linked to a ticket or TODO.

These markers are the coverage contract. They should not contain resource
business logic. They only answer: does this resource family have validation
parity, is validation not applicable, or is work still outstanding?

Avoid no-op defaults that look like success. A new resource family should fail a
local parity audit until it chooses one of these states and, when implemented,
points at resource-local validation code.

### 2. Typed Local Resource Parsing

Build typed local resource parsers from the same discovered resource mapping
data used by `validate_project` in Python. Resource-family parsing should sit
behind a standard trait, currently shaped as:

```rust
trait ParseLocalResource {
    type Parsed;

    fn parse_local_yaml(path: &str, yaml: &serde_yaml_ng::Value)
        -> Result<Self::Parsed, ResourceParseErrors>;
}
```

The parser should return a typed resource-family value whose fields encode as
many resource-local invariants as practical. Prefer deserializing newtypes and
enums over post-parse `Value` traversal:

- `NonEmptyString` for required non-empty names and regex strings;
- `NoEdgeWhitespace` for Python's description whitespace checks;
- Rust enums for allowed string values such as SIP methods, replacement types,
  entity types, and ASR interaction styles;
- collection wrappers or parse constructors for duplicate names, exactly-one
  default resources, and complete variant coverage.

Only use raw intermediary structs when the Python authoring shape has defaults
or compatibility behavior that cannot be expressed cleanly in the final type.
The raw type should be private to the resource family and immediately converted
into the parsed type.

The parser inputs include:

- singletons such as role, personality, ASR settings, channel configuration,
  and safety filters;
- aggregate entries such as API integrations, handoffs, SMS templates, variants,
  keyphrases, pronunciations, and transcript corrections;
- per-resource files such as topics, functions, flow configs, and flow steps.

Typed parsing should preserve the local authoring semantics that Python uses:
resource names are converted to IDs before validation, flow-scoped resources
know their flow ID/name, and aggregate entries are validated as individual
resources before collection checks run.

`DiscoverResources::validate_local_yaml` should become a thin compatibility
adapter over typed parsing while the migration is in progress. It should not be
the place where resource-family business rules are authored.

### 3. Shared Reference Validation

Port Python's `validate_references()` behavior as a shared Rust helper. It
should validate `{{prefix:name-or-id}}` references against the discovered
resource mapping context, with flow scoping where Python applies it.

Resource validators should declare the allowed reference families for each
field. Examples:

- topics: global functions, SMS, handoffs, attributes, variables, translations;
- rules: global functions, SMS, handoffs, attributes, variables, translations;
- role/personality: attributes and variables;
- channel greetings/disclaimers: attributes, variables, translations;
- SMS templates: variables and translations;
- flow prompts: global functions, transition functions, attributes, variables,
  translations, and flow-local scope rules.

### 4. Resource Validators

Each resource-family module should own its Python parity rules, including both
per-resource and collection checks. Examples from DEVP-319:

- handoffs: SIP method, invite encryption, exactly one default handoff;
- API integrations: environment URL/auth rules, operation rules, duplicate
  operation detection;
- safety filters: enabled/category shape, category booleans, level values;
- variants: one default variant and complete attribute coverage;
- functions: description rules, latency control bounds, flow references,
  `goto_step` and `goto_flow` validation;
- transcript corrections: per-rule regex, replacement type, duplicate names.

Cross-resource orchestration should stay in `adk-core/src/validation.rs`, but
the actual resource-family semantics should live in `adk-resources`.
`adk-core` should coordinate the validation phases and provide shared context;
it should not become the home for handoff, SMS, API integration, or channel
setting rules.

### 5. Error Shape Compatibility

Prefer Python-compatible error strings where tests already record or assert
them. When exact wording is not practical, keep these stable:

- resource path;
- resource family/name when available;
- field or collection item location;
- Python concept name such as `replacement_type`, `interaction_style`, or
  `flow.goto_step`.

Avoid returning a generic parse/protobuf failure when Python would report a
resource validation error.

## Migration Plan

### Phase 1: Make Coverage Visible

- Add inline validation parity markers for every resource type in
  `RESOURCE_TYPE_REGISTRY`.
- Include the Python validator being audited, parity status, and owning ticket
  or TODO in the marker.
- Mark all DEVP-319 gaps as `todo` until implemented.
- Optionally add a lightweight test or script that checks every registered
  resource type has a validation parity marker.

### Phase 2: Add the Shared Context

- Build a Rust equivalent of Python's `ResourceMapping` context for validation.
- Include resource type, ID, local name, local file path, resource prefix, and
  flow name where applicable.
- Reuse existing discovery facts instead of adding validation-only discovery.
- Add helper APIs for resolving names to IDs and validating reference sets.

### Phase 3: Port Collection and Simple Resource Parsers

Start with low-dependency parsers to exercise the architecture:

- handoffs;
- keyphrase boosting;
- pronunciations;
- ASR settings;
- transcript corrections;
- safety filters.

These validate mostly one file or one aggregate collection and should give quick
coverage without depending heavily on prompt reference resolution.

Add a curated parse matrix for this phase. Each row should feed one invalid
fixture through `ParseLocalResource::parse_local_content` and assert the
specific invariant that fails. Prefer one invariant per fixture so the tests
survive implementation changes from YAML traversal to typed parsing.

### Phase 4: Port Reference-Aware Validators

Add validators that depend on resource mappings and name-to-ID replacement:

- topics;
- rules, role, and personality;
- SMS templates;
- channel settings;
- phrase filters;
- variant attributes;
- flow prompt references and extracted entities.

This phase should make the shared reference helper the only path for prompt
reference validation, rather than duplicating parser logic in each family.

### Phase 5: Port Function and Flow Edge Cases

Finish the parity-sensitive Python and flow checks:

- function description requirements and whitespace;
- transition function flow ID requirements;
- latency control bounds and required delay responses;
- `flow.goto_step()` and `conv.goto_flow()` validation;
- flow step filename matching;
- flow config existence and flow ID validation;
- condition type enum validation.

Keep existing syntax/signature validation, but route new checks through the
typed validation context so global functions, transition functions, and function
steps share the same metadata model as Python.

### Phase 6: Remove Silent No-Ops

After each family has an explicit parity status, remove or quarantine the
current no-op validation default. New resource support should not compile or
should fail a parity matrix test until validation status is explicit.

## Test Strategy

- Add focused in-memory tests for each resource validator and collection
  validator.
- Add CLI `poly validate --json` tests for representative invalid local
  projects.
- Add Python ADK recording fixtures only when the output shape or behavior is
  hard to infer from local Python tests.
- For each DEVP-319 bullet, add either a Rust parity test or a documented TODO
  entry with the owning module.

Tests should assert both `poly validate` and `poly push --dry-run` behavior when
validation blocks user-facing workflows.

## Review Checklist

Before closing a validation parity PR:

- Does every changed resource family name the Python validator it mirrors?
- Are per-resource and collection-level checks both covered where Python has
  both?
- Does the implementation validate local authoring names after Python-compatible
  name-to-ID conversion?
- Are invalid references validated through the shared reference helper?
- Does `push` fail before command generation unless `--skip-validation` is set?
- Are any intentional Python divergences called out in the PR summary?

## Tracking

Use DEVP-319 as the umbrella ticket. Smaller implementation PRs should update
the inline validation parity markers as they move resource families from `todo`
to `implemented`.
