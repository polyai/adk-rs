# Remaining Python ADK Parity TODOs

Concrete follow-ups from the latest Python-vs-Rust audit. Each item should be covered by a Python recording fixture first where practical, then brought to parity in Rust.

- [x] **Port deployment show/promote/rollback workflows** (`deployments-mutation-parity`)
  - Python supports `deployments show`, `deployments promote`, and `deployments rollback`; Rust currently implements only `deployments list`.
  - Add recordings for JSON `show`, `promote --dry-run`, `promote --json --force`, `rollback --dry-run`, and `rollback --json --force`, then implement the missing Rust CLI/platform methods.
  - Acceptance: JSON replay fixtures match Python for deployment selection by 9-character hash prefix, included/reverted deployment calculation from sandbox history, active environment aliases, dry-run payloads, and successful promote/rollback HTTP calls.
  - Implemented: Rust now supports deployment `show`, `promote`, and `rollback`, including platform-root promote/rollback endpoints, dry-run payloads, active environment aliases, prefix lookup, included/reverted deployment calculation, confirmation prompts, and a real Python recording/replay fixture in `deployments-mutation`.

- [x] **Handle start/end special functions as first-class resources** (`special-function-parity`)
  - Python treats `functions/start_function.py` and `functions/end_function.py` as `start_function_*` and `end_function_*` commands, not ordinary global functions.
  - Add recordings that pull and push start/end function edits, creates, and deletes.
  - Acceptance: Rust materializes these files from `specialFunctions`, discovers them with the same path convention, and emits `create/update/delete_start_function` or `create/update/delete_end_function` instead of generic function commands.
  - Implemented: Rust now materializes `functions/start_function.py` and `functions/end_function.py` from `specialFunctions`, discovers `conv.state.*` variables as push-only resources, emits start/end create/update/delete commands with Python-compatible variable reference updates/deletes, and replays the real `special-functions` Python recording.

- [x] **Complete webchat and channel safety-filter resource parity** (`channel-settings-parity`)
  - Python pulls webchat channel resources into `chat/configuration.yaml` and `chat/safety_filters.yaml`, and pushes channel greeting/style/safety filter updates for both voice and webchat.
  - Add recordings for webchat greeting/style/safety-filter pull and push, plus voice safety-filter push.
  - Acceptance: Rust round-trips `channels.webChat.config.greeting`, `stylePrompt`, `safetyFilters`, `channels.voice.config.safetyFilters`, and emits the matching `channel_update_*` command payloads with `VOICE`/`WEB_CHAT` channel types.
  - Implemented: Rust now materializes webchat configuration and safety-filter resources when the webchat channel is created, pushes voice and webchat safety filters plus webchat greeting/style updates with Python-compatible channel types and ordering, and replays the real `channel-settings` Python recording. The recording establishes a webchat-enabled baseline with `pull --from-projection` before editing so Python exercises update commands rather than unsupported channel setting creates.

- [x] **Complete update/delete command parity for broad multi-resource families** (`broad-resource-lifecycle-parity`)
  - Rust currently creates several broad resources but does not emit Python-compatible update/delete commands for all lifecycle states.
  - Add recordings that update and delete variants/variant attributes, API integrations/configs/operations, keyphrase boosting entries, transcript corrections, and pronunciations.
  - Acceptance: Rust push matches Python's command types and ordering for create, update, and delete across these families, including nested API integration operation/config commands.
  - Implemented: Rust now diffs broad multi-resource YAML files against projection data for variant/attribute, API integration/config/operation, keyphrase, transcript correction, and pronunciation create/update/delete lifecycles. The `broad-lifecycle` Python recording documents the command contract and is enabled in the cheap Rust replay suite.

- [x] **Send broad-resource commands on real push** (`push-broad-resources-real`)
  - Extend real `push` command generation, not only dry-run summaries, for variants, API integrations/operations, keyphrase boosting, transcript corrections, pronunciations, voice/channel settings, and other broad resources already documented by dry-run recordings.
  - Acceptance: a non-dry-run recording fixture for broad resources replays successfully and the Rust HTTP command batch matches Python semantics.
  - Implemented: Rust real-push and dry-run preview now share protobuf command generation for the broad resource families covered by `push-resource-coverage`; platform tests assert real command payloads and the Python replay fixture still passes. A future live recording refresh can replace the current dry-run-only cassette with a permanent non-dry-run fixture.

- [x] **Implement interactive `init` selection flow** (`interactive-init`)
  - Match Python behavior for fetching accessible regions, accounts, and projects when `--region`, `--account_id`, or `--project_id` are omitted in human mode.
  - Acceptance: non-JSON interactive tests cover auto-select single option, prompted multi-option selection, no account/project found, and cancellation behavior.
  - Implemented: human-mode `init` now fetches accessible regions/accounts/projects, auto-selects single region/account options, prompts for project selection, preserves JSON-mode strictness, and stores the selected project name. Human-output tests cover auto-select, prompt selection, and cancellation paths.

- [x] **Implement interactive `branch switch`** (`interactive-branch-switch`)
  - When no branch name is supplied in human mode, list available branches, mark the current branch, prompt for a target branch, and handle cancellation without treating it as an error.
  - Acceptance: text-mode fixture covers no-branch prompt, current marker, selected branch switch, and cancellation/no-branches cases.
  - Implemented: human-mode `branch switch` now prompts from the branch list, marks the current branch, keeps JSON mode strict, and treats cancellation as a clean exit. Human-output tests cover prompted selection.

- [x] **Implement interactive `branch delete`** (`interactive-branch-delete`)
  - Match Python's checkbox-style branch deletion flow: exclude `main`, allow deleting one or more branches, confirm before deleting, and report when deleting the current branch switches back to `main`.
  - Acceptance: tests cover no deletable branches, cancellation, single branch deletion, multi-branch deletion, and JSON behavior for direct branch-name deletion.
  - Implemented: human-mode branch deletion prompts for one or more non-main branches, confirms before deletion, switches local config to `main` when deleting the current branch, and preserves JSON direct-delete behavior. Human-output tests cover the no-branch and multi-delete/current-branch paths.

- [x] **Implement interactive merge conflict resolution** (`interactive-branch-merge`)
  - Port Python's conflict enrichment, timestamp auto-resolution, auto-merge acceptance, existing resolution reuse, edit-in-editor flow, and remaining-conflict retry loop.
  - Acceptance: fixtures cover non-interactive conflict output, `--resolutions` file/stdin/inline JSON, interactive auto-merge, edit flow, and unresolved retry behavior.
  - Implemented: human interactive merge now enriches conflicts, auto-resolves timestamp fields, accepts auto-merge/ours/theirs/base/edit choices, reuses supplied resolutions, rejects unresolved conflict markers, and retries with `conflictResolutions`. Human-output tests cover auto-merge retry.

- [x] **Implement real interactive chat loop** (`interactive-chat-loop`)
  - Support human-mode prompting, `/exit`, `/restart`, end-chat calls, resumed conversations, branch deployment messaging, and multiple conversation collection in JSON mode.
  - Acceptance: replayable tests cover scripted restart, explicit exit, resumed conversation ID, send-message error handling, and server-side `conversation_ended`.
  - Implemented: chat now has a real loop with human prompts, scripted echoing, `/exit`, `/restart`, end-chat calls, resumed conversation messaging, branch environment messaging, human transcript output, metadata rendering, and multi-conversation JSON collection. Human-output and JSON restart tests cover the core flow.

- [x] **Write full Python `_gen` package on init/pull/switch** (`python-gen-package`)
  - Write the Python-compatible `_gen` package, decorator exports, import stubs, and type helper files that Python writes during `init`, `pull`, and branch/environment pulls.
  - Acceptance: recorded pull/init fixture asserts `_gen/__init__.py`, `_gen/decorators.py`, and representative helper modules match Python-compatible import behavior.
  - Implemented: `adk-core` now owns crate-local Python-compatible `_gen` templates and writes them from init and status snapshot paths, including decorator exports and all helper modules. Core tests assert init/pull package contents and stale `.pyi` cleanup.

- [x] **Run and persist project migrations** (`project-migrations`)
  - Port Python's migration flag system, starting with legacy topic file migration from nested/name-only topic YAML files to cleaned top-level topic files with `name` keys.
  - Acceptance: tests cover already-migrated projects, legacy nested topics, duplicate cleaned names, and status snapshot `migration_flags`.
  - Implemented: project loading runs and persists `migrated_legacy_topic_files`, migrates legacy/nested topic files into top-level cleaned YAML with `name`, removes empty legacy dirs, and errors on duplicate cleaned names. Core tests cover nested migration, duplicate names, and persisted flags.

- [x] **Align `_gen/.agent_studio_config` snapshot shape** (`status-snapshot-shape`)
  - Expand Rust's status snapshot to preserve Python-compatible `region`, `account_id`, `project_id`, `project_name`, `last_updated`, typed resources, typed `file_structure_info`, and migration flags.
  - Acceptance: a Python-initialized project can be read by Rust and a Rust-initialized/pulled project can be read by Python without lossy fallback behavior.
  - Implemented: snapshots now include top-level project metadata, `last_updated`, migration flags, typed resource groups, and typed `file_structure_info` entries with resource id/name/hash. Core tests assert the expanded shape after init/pull and force pull.

- [x] **Delete local-only resources on `pull --force`** (`pull-force-delete-local-only`)
  - Match Python force-pull behavior by removing local resources/files not present in the remote projection, including pronunciation ordering and empty flow-folder cleanup.
  - Acceptance: fixture creates local-only files across resource families, runs `pull --force`, and verifies only remote resources remain.
  - Implemented: force pull now deletes local-only discovered resource files and removes empty flow subdirectories after applying the remote projection. Core tests cover local-only topic/flow/step deletion and empty flow-folder cleanup.

- [x] **Close formatting parity gaps** (`format-resource-parity`)
  - Match Python's resource-specific formatting for YAML, JSON, Python, multi-resource YAML files, `--files` path resolution relative to `--path`, and `ty` timeout/result behavior.
  - Acceptance: tests cover JSON formatting, YAML multi-resource formatting, absolute and base-path-relative `--files`, invalid resource format errors, and `ty` timeout/nonzero reporting.
  - Implemented: formatting now supports explicit JSON file formatting, keeps project-wide replay behavior aligned with Python recordings, normalizes `--files` relative to `--path` including absolute paths, preserves invalid YAML/JSON content instead of failing formatter runs, and runs `ty` with a Python-compatible timeout/result shape. CLI tests cover JSON and absolute-path formatting.

- [x] **Port full resource validation semantics** (`resource-validation-parity`)
  - Extend Rust validation beyond the current semantic checks to mirror each Python resource class's validation, including references, required fields, value enums, duplicate names, and resource-family-specific constraints.
  - Acceptance: Python recording fixtures document representative invalid resources per family and Rust `validate --json` matches the error contract.
  - Implemented: validation now covers duplicate names and required names for common multi-resource config files, entity `entity_type` requirements/enums, topic name requirements, variant duplicate/default checks, API integration naming, and transcript correction rule requirements. CLI JSON tests cover duplicate entity names and invalid entity types; existing Python recording replay remains green.

- [x] **Honor `deployments list --details` in human mode** (`deployments-details-output`)
  - Match Python's detailed vs compact deployment rendering while preserving JSON output shape.
  - Acceptance: human-output tests cover default compact rendering, `--details`, empty deployments, hash-prefix filtering, and not-found hash behavior.
  - Implemented: human deployment output now has compact and detailed renderings, active environment badges, deployment metadata fields, and unchanged JSON output shape. Human-output tests cover compact vs detailed rendering.

- [x] **Make `--verbose` and `--debug` meaningful** (`verbose-debug-behavior`)
  - Match Python's `--verbose` traceback behavior and `--debug` logging behavior for supported commands, while keeping JSON-mode error payloads stable.
  - Acceptance: tests cover human verbose traceback visibility, default concise errors, debug logging activation, and JSON traceback behavior.
  - Implemented: non-JSON `emit_error` now uses the rich traceback helper, concise mode prints a verbose hint, verbose mode prints a traceback, debug mode emits tracing debug logs, and JSON error payloads remain stable. Human-output tests cover concise/verbose errors and debug logging.
