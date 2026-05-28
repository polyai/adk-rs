# Current TODOs

Short-term engineering notes for the next few refactor passes. Keep this file
small and current; historical parity context belongs in
`docs/python-adk-parity-history.md`.

## Resource Organization

Goal: keep `adk-core` focused on workflow orchestration while `adk-resources`
owns resource-family semantics.

Current state:

- Status snapshot payload and hash construction moved into `adk-resources`.
- Python function resource helpers used by status snapshots moved into
  `adk-resources`.
- `adk-core` still owns when snapshots are loaded/written, but no longer owns
  the per-resource rules for payload shape or hash normalization.
- Push command generation entrypoints use the current `build_push_commands`
  naming; the old `phase1` terminology has been removed.
- Resource-specific tests have been moved closer to their implementation for
  functions, flows, singleton files, aggregate files, status snapshots, and
  related materialization helpers. `adk-resources/src/tests.rs` is now reserved
  mostly for cross-resource orchestration and broad projection coverage.
- The first durable function-family module exists at `adk-resources/src/functions`.
  It owns function discovery, Python parsing/decorator helpers, legacy status
  compatibility helpers, projection materialization, validation helpers, and
  push command generation.
- The flow-family module at `adk-resources/src/flows` owns flow discovery,
  projection materialization, cross-resource validation, and push command
  generation for flow config, steps, function steps, and transition functions.
- The topic-family module at `adk-resources/src/topics` owns topic discovery,
  local YAML validation, projection materialization, and push command
  generation.
- The variable-family module at `adk-resources/src/variables` owns virtual
  variable discovery from Python `conv.state.*` usage and variable push command
  generation.
- The API integration-family module at `adk-resources/src/api_integrations`
  owns aggregate-file discovery and validation, projection materialization, push
  command generation, and command JSON summaries for API integrations.
- The variant-family module at `adk-resources/src/variants` owns aggregate-file
  discovery and validation, projection materialization, push command generation,
  and command JSON summaries for variants and variant attributes.
- The keyphrase boosting, transcript corrections, and pronunciations modules
  own their aggregate-file discovery, projection materialization, and push
  command generation. Transcript corrections also owns its local YAML
  validation and command JSON summaries.
- The handoff-family module at `adk-resources/src/handoffs` owns aggregate-file
  discovery and validation, projection materialization, push command
  generation, default-selection post-update ordering, and command JSON
  summaries.
- The SMS template and phrase filter modules own their aggregate-file
  discovery, projection materialization, push command generation, and command
  JSON summaries. SMS templates also owns local YAML validation.
- The entity-family module at `adk-resources/src/entities` owns aggregate-file
  discovery and validation, projection materialization, push command
  generation, entity config conversion, and command JSON summaries.
- The experimental-config module at `adk-resources/src/experimental_config`
  owns singleton discovery, projection materialization, push command generation,
  and command JSON summaries for `agent_settings/experimental_config.json`.
- The agent-settings module at `adk-resources/src/agent_settings` owns
  discovery, projection materialization, push command generation, and command
  JSON summaries for personality, role, rules, and agent-level safety filters.
- The channel and ASR modules at `adk-resources/src/channels` and
  `adk-resources/src/asr_settings` own discovery, projection materialization,
  push command generation, and command JSON summaries for voice/chat channel
  settings, channel safety filters, and ASR settings.
- The command-generation orchestration no longer uses layout-named dispatcher
  modules. A neutral command queue preserves cross-family ordering while JSON
  summaries route directly to resource-family helpers.

Remaining:

1. Clean up the remaining orchestration layer now that resource-family modules
   own the resource-specific behavior.
   - The operation-oriented modules (`local_resources`, `materialization`, and
     `push_commands`) are now mostly dispatchers, queue construction, and shared
     helpers.
   - Keep these modules only where they clarify ordering, global command queue
     semantics, cross-resource reference rewriting, or typed lifecycle
     bookkeeping.
   - `functions`, `flows`, `topics`, `variables`, `entities`,
     `experimental_config`, `agent_settings`, `asr_settings`, `channels`,
     `api_integrations`, `variants`, `keyphrase_boosting`,
     `transcript_corrections`, `pronunciations`, `handoffs`, `sms_templates`,
     and `phrase_filters` now follow the resource-family shape.
   - The local layout taxonomy (`singletons`, `aggregates`,
     `per_resource_files`) remains descriptive vocabulary only, not a command
     generation module boundary.
   - Use the colocated tests as a guide for the production module boundaries:
     resource-family semantics should move together, while broad orchestration
     tests stay in umbrella modules.

## Watchpoints

- Preserve Python ADK parity and replay coverage while moving code.
- Avoid changing generated project file formatting or status snapshot hashes
  unless that is the explicit goal of the change.
- Prefer small mechanical moves before semantic cleanup.
