# Python ADK Parity History

This file is the short institutional memory for how the Rust ADK reached Python
ADK feature parity. It is intentionally not a full backlog. Use it to understand
why some code paths preserve odd Python behavior, why the replay fixtures matter,
and where future agents should be careful.

## Guiding Principles

- Python ADK behavior is the contract, even when it is surprising.
- Record Python behavior first when practical, then make Rust replay it.
- Keep local project files stable. Users commit these files and rely on clean
  diffs to inspect real Agent Studio sync changes.
- Prefer mechanical refactors before semantic changes. Parity-sensitive behavior
  should stay covered by replay fixtures or focused local tests.

## Parity Milestones

### Project Shape And Snapshots

- Rust now writes the Python-compatible `_gen` package on init, pull, and branch
  switch, including decorators, import stubs, and helper modules.
- `_gen/.agent_studio_config` now matches Python's richer snapshot shape:
  region/account/project metadata, `last_updated`, migration flags, typed
  resources, and typed `file_structure_info`.
- Project migrations run and persist flags, starting with legacy nested topic
  files moving to cleaned top-level topic YAML with `name` keys.
- `pull --force` removes local-only resources and cleans empty flow folders, so a
  forced pull really mirrors the remote projection.

### Projection And Resource Materialization

- Projection materialization rejects duplicate cleaned file paths instead of
  silently overwriting resources.
- Prompt references are materialized as local names (`fn`, flow-scoped `ft`,
  `attr`, and `vrbl`) and converted back to IDs during push.
- Materialization covers the major resource families: topics, entities, global
  functions, start/end functions, flow resources, transition functions, broad
  YAML resources, channel settings, handoffs, SMS templates, phrase filtering,
  experimental config, and agent settings.
- YAML materialization for representative resources uses ordered serde models to
  avoid Python-vs-Rust key-order churn.

### Function And Flow Semantics

- Function decorators are parsed for descriptions, parameters, latency control,
  variable references, and Python-compatible schema types.
- Start and end functions are first-class resources, not generic functions.
- Flow transition functions under `flows/<flow>/functions/*.py` are materialized,
  validated with `Conversation, Flow`, and pushed with create/update/delete plus
  latency-control updates.
- Flow lifecycle parity is covered for flow config, steps, function steps,
  no-code conditions, whole-flow deletion, status/diff classification, and
  validation failures.
- Rust validates Python syntax with a pure-Rust Ruff-lineage parser so validation
  does not require a Python interpreter.

### Push, Pull, Status, Diff, And Validation

- Broad multi-resource lifecycle commands now match Python for variants,
  attributes, API integrations/configs/operations, keyphrase boosting,
  transcript corrections, and pronunciations.
- Real push and dry-run push share command generation for broad resources and
  channel/agent settings.
- Resource validation covers common family-specific constraints: duplicate names,
  required fields, enums, references, variants/defaults, API integration naming,
  entities, topics, and transcript corrections.
- Python recording fixtures document the important contracts; replay tests are
  the main guardrail for JSON output shape and command ordering.

### CLI Behavior

- `init --format`, `pull --format`, and `branch switch --format` apply formatting
  while writing resources and baseline snapshots from the formatted disk state.
- Hidden projection flags behave like Python: `--output-json-projection` captures
  projection output without forcing JSON-only argument/error semantics.
- Remote-deleted current branches resolve to `current_branch: null`, warn in
  human mode, and allow pull to reconcile the local config to the server-selected
  branch.
- `chat --input-file` intentionally preserves the recorded Python failure
  contract. The Python source is annotated because the behavior appears buggy.
- `--verbose` and `--debug` now have Python-compatible effects for human errors,
  tracebacks, debug logging, and stable JSON error payloads.

### Interactive And Remote Workflows

- Human-mode `init`, `branch switch`, and `branch delete` now prompt like Python,
  including auto-selection, cancellation, current-branch markers, multi-delete,
  and switching back to `main` when deleting the current branch.
- Interactive branch merge ports Python's conflict enrichment, timestamp
  auto-resolution, auto-merge choices, edit flow, supplied resolutions, and retry
  loop for unresolved conflicts.
- Chat now has a real human loop with `/exit`, `/restart`, end-chat calls,
  resumed conversations, branch deployment messaging, transcript output, and
  multi-conversation JSON collection.
- Deployments now support `list --details`, `show`, `promote`, and `rollback`,
  including dry-run payloads, active environment aliases, prefix lookup, and
  platform-root mutation endpoints.

### Code Organization

- `adk-core/src/lib.rs` is a facade; Python helpers, status snapshots,
  validation, `AdkService`, and `ProjectWorkspace` live in focused modules.
- CLI command handling is split into command modules, with `main.rs` reduced to
  parser dispatch and shared service/prompt helpers. CLI args and `revert` are
  split out as well. `review` remains visible but is explicitly marked
  incomplete; the old token-error recording was not a real parity contract.
- `adk-push-pull` materialization keeps `projection_to_resource_map` as the
  facade, with broad resources, channels, synthetic resources, agent settings,
  flows, functions, topics, entities, and references in focused modules.
- Structured single-file command generation keeps a small aggregation entrypoint
  and splits variants, API integrations, keyphrases, transcript corrections,
  pronunciations, settings, summaries, and common helpers.

## Current Watchpoints

- YAML formatting remains the easiest place to create noisy diffs. Python's
  ruamel-based formatter preserves insertion order, literal block scalars, some
  quoting choices, and wrapping behavior that broad `serde_yaml` formatting does
  not always match.
- Large files that remain are mostly refactor targets, not parity blockers:
  `adk-core/src/service.rs` and the flow command-generation internals.
- When adding new resource-family behavior, prefer a Python recording fixture
  plus replay coverage over purely synthetic tests unless the behavior is
  inherently local-only.
