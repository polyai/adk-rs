
# Remaining Python ADK Parity TODOs

Concrete follow-ups from the latest Python-vs-Rust audit. Each item should be covered by a Python recording fixture first where practical, then brought to parity in Rust.

- [ ] **Modularize large Rust implementation files** (`code-quality-modularization`)
  - Keep `adk-core/src/lib.rs` as a facade and move unrelated implementation clusters into focused modules.
  - Core sequence done: Python helpers, status snapshot helpers, validation, `AdkService`, and `ProjectWorkspace` now live in focused modules.
  - Follow-up progress: command JSON summaries, CLI completion, CLI self-update, CLI deployments, CLI branch, CLI chat, shared CLI output helpers, `adk-push-pull` materialization, command generation, and function parsing are now split into focused modules.
  - Next CLI target: split `adk-cli/src/main.rs` by command, starting with `init`, `pull`, `push`, `status`, `diff`, `validate`, `format`, `review`, and `project`.
  - Next materialization target: split `adk-push-pull/src/materialization/mod.rs` by resource family, keeping `projection_to_resource_map` as the facade. Candidate modules: `flows`, `functions`, `topics`, `single_file`, `channels`, `entities`, and `broad_resources`.
  - Next command-generation targets: split `command_gen/flows.rs` into `flow_config`, `steps`, `function_steps`, `transition_functions`, `conditions`, and shared YAML helpers; split `single_file_resources/structured.rs` into `variants`, `api_integrations`, `keyphrases`, `transcript_corrections`, `pronunciations`, and `channel_settings`.
  - Later core target: split `adk-core/src/service.rs` by workflow after CLI and push/pull boundaries settle.
  - Keep each slice mechanical first: move code, preserve signatures/behavior, and run the existing tests before making semantic changes.

- [x] **Restore flow transition-function parity** (`flow-transition-function-parity`)
  - Python treats `flows/<flow>/functions/*.py` as transition-function resources, materializes them from remote flow transition functions, validates them with `Conversation, Flow` signatures, and pushes create/update/delete transition-function commands plus latency-control updates.
  - Rust currently discovers these files but does not materialize transition functions from projections, and `push_functions.rs` ignores paths outside `functions/`, so local transition-function changes can be invisible to push.
  - Add Python recording coverage for pull/init materialization, status/diff, validation, dry-run push create/update/delete, and latency-control updates, then implement Rust projection and command generation.
  - Progress: the in-memory parity matrix now covers transition-function materialization plus create/update/delete command generation, and Rust now materializes `flows/<flow>/functions/*.py`, embeds transition functions in new-flow creates, and emits existing-flow transition-function create/update/delete commands.
  - Implemented: transition functions are now covered by the local parity matrix for materialization/create/update/delete, validated with flow-scoped `Conversation, Flow` signatures, and generate flow-scoped latency-control update commands from `@func_latency_control` decorators.

- [x] **Finish Python function decorator, metadata, and validation parity** (`function-decorator-validation-parity`)
  - Python parses `@func_description`, `@func_parameter`, and `@func_latency_control`, preserves existing parameter and delay-response IDs, maps Python annotations to schema types, extracts variable references, and validates exact function shapes and references.
  - Rust currently infers global function parameters from signatures with empty descriptions/string types, omits global function variable references and latency-control commands, gives function steps default latency-control payloads, and only validates a narrow subset of function semantics.
  - Add recordings for global, start, end, transition, and function-step decorator metadata plus representative invalid-function cases, then port the parser and validator behavior.
  - Progress: global/transition function command generation now parses simple `@func_parameter` decorators, maps `str`/`int`/`float`/`bool` annotations to Python-compatible schema types, ignores `conv`/`flow` receiver parameters, preserves remote parameter metadata objects, and includes variable references on global function create/update commands.
  - Implemented: `@func_latency_control` is now parsed for global functions, flow function steps, and transition functions; global creates embed latency control, global updates emit `update_latency_control`, function-step creates/updates carry Python-compatible latency payloads, and transition functions emit flow-scoped latency-control updates. Generated parameter and delay-response IDs are normalized in recording replay, decorator parameter/type validation is covered locally, and function-step create payloads preserve Python's empty latency-control object shape when latency is disabled.

- [x] **Honor formatting flags on init, pull, and branch switch** (`init-pull-switch-format-parity`)
  - Python passes `--format` through `init_project`, `pull_project`, and `switch_branch`, formatting resources as they are written.
  - Rust exposes the same flags but currently only uses formatting for `push --format` and `format`; `init --format`, `pull --format`, and `branch switch --format` are ignored.
  - Add focused recording fixtures for each command and make the write path apply Python-compatible resource formatting.
  - Implemented: `init --format`, `pull --format`, and `branch switch --format` now route through the shared formatted pull writer, format written resources, and baseline the status snapshot from formatted disk content so a freshly formatted pull stays clean. Local projection-driven CLI tests cover all three commands without expanding the cassette set.

- [x] **Align hidden projection-output CLI semantics** (`projection-output-flag-parity`)
  - Python treats `--output-json-projection` as an output/capture flag, not as full `--json`: interactive `init` and `branch switch` can still prompt, missing-project errors follow human mode unless `--json` is set, and branch switch returns the pulled projection when no `--from-projection` is supplied.
  - Rust currently folds `output_json_projection` into JSON-mode selection/error behavior and returns `projection: null` for branch switch without `--from-projection`.
  - Add replay and recording coverage for `init`, `pull`, and `branch switch` hidden projection flags, then align prompts, errors, and projection payloads.
  - Implemented: projection-output mode no longer forces JSON-only init/branch-switch argument behavior or missing-project errors, and remote branch switch now fetches and returns the selected branch projection when `--output-json-projection` is used without `--from-projection`. Local CLI tests cover missing-project error mode and non-null branch-switch projection output.

- [x] **Reconcile remote-deleted current branch behavior** (`remote-deleted-branch-parity`)
  - Python reports `current_branch: null` and human warnings when the locally configured branch id no longer exists remotely, and `pull` can switch to the server-selected branch while emitting `new_branch_name` and `new_branch_id`.
  - Rust currently falls back to displaying the stale local branch id, includes it in branch lists, and `pull` does not update or report branch changes from the remote client.
  - Add recording coverage for branch current/list/pull after remote branch deletion, then align branch reconciliation.
  - Implemented: branch current/list now treat a missing remote branch as `None` instead of inventing a stale branch entry, human mode warns about the deleted/merged local branch, and pull reconciles to `main` (or the first available branch) while updating `project.yaml` and emitting `new_branch_name`/`new_branch_id` in JSON output.

- [x] **Detect duplicate projected file-path collisions** (`duplicate-projection-path-parity`)
  - Python raises `Duplicate resource file path found...` when remote/projection resources clean to the same local path.
  - Rust projection materialization is keyed by file path and can silently overwrite one colliding resource.
  - Add projection/unit coverage for duplicate cleaned names across single-file and multi-resource families, then fail before writing.
  - Implemented: projection materialization now uses a single checked insertion path, raises the Python-style duplicate path error before writing, and aligns platform projection-name cleaning with Python's punctuation/whitespace normalization. Platform tests cover single-file topic collisions and flow-step path collisions.

- [x] **Record and decide `chat --input-file` behavior** (`chat-input-file-parity`)
  - Python dispatch currently appears to treat the loaded input-file string as a context manager before splitting lines, while Rust implements the intended line-splitting behavior.
  - Add a Python behavior recording or annotate the Python bug before deciding whether Rust should preserve the observed contract or intentionally differ.
  - Implemented: `chat-json` records the observed Python input-file failure contract, the Python source now calls out the bug explicitly, and Rust preserves the same behavior while still matching Python's file-not-found check before the context-manager failure.

- [x] **Close resource-family recording coverage gaps** (`resource-family-recording-coverage`)
  - Our Python recordings are high-fidelity contracts for the workflows they exercise, but they are not yet a complete resource-family coverage matrix.
  - Add Python recording fixtures for flow resources (`flow_config`, `flow_steps`, `function_steps`) covering pull/init materialization, status/diff detection, dry-run push create/update/delete, and validation errors.
  - Add pull/init materialization recordings for broad multi-resource files that currently have push coverage but weak pull evidence: `config/api_integrations.yaml`, `config/variant_attributes.yaml`, `voice/speech_recognition/keyphrase_boosting.yaml`, `voice/speech_recognition/transcript_corrections.yaml`, and `voice/response_control/pronunciations.yaml`.
  - Add Python recording coverage for currently synthetic-only resource families: `entities`, `experimental_config`, `sms_templates`, `handoffs`, and `phrase_filtering`.
  - Acceptance: every Python `RESOURCE_NAME_TO_CLASS` family has explicit recording evidence for the behavior it supports; Rust replay tests fail if any covered resource family is not materialized, discovered, validated, or translated into Python-compatible commands.
  - Progress: added and enabled `flow-resource-coverage`, a local-only Python recording for create-flow dry-run command generation across `flow_config`, advanced/default `flow_steps`, `function_steps`, and no-code exit conditions. Rust now emits matching `create_flow`, `create_step`, and `create_no_code_condition` JSON/protobuf commands for this create path.
  - Progress: added and enabled `resource-materialization`, a local-only Python recording that asserts pull materialization for flows, broad multi-resource files, and synthetic interaction/config families. Rust now materializes the covered flow, variant, API integration, keyphrase, transcript correction, pronunciation, entity, experimental config, SMS, handoff, and phrase-filter files.
  - Progress: added and enabled `synthetic-lifecycle`, a local-only Python recording for create/update/delete dry-run command generation across `entities`, `experimental_config`, `sms_templates`, `handoffs`, and `phrase_filtering`. Rust replay now matches Python's JSON command contract, including generated ID mapping, update summaries, default handoff post-update behavior, and unchanged transcript-correction suppression.
  - [x] Add a focused flow lifecycle recording for update/delete behavior across `flow_config`, `steps/*.yaml`, `function_steps/*.py`, and no-code conditions, then make Rust dry-run command output match Python.
  - [x] Add flow status/diff recording coverage for modified, new, and deleted flow files, then align Rust path classification and output ordering.
  - [x] Add flow validation recording coverage for representative invalid flow resources: missing/unknown start step, malformed condition config, bad references, and malformed function-step shape.
  - [x] Add focused flow deletion and no-code condition deletion coverage; current enabled recordings cover flow create, flow config update, advanced/default step update, function-step create/delete, and no-code condition create/update/delete.
  - [x] Add a permanent, narrow non-dry-run live-push fixture for representative broad/flow resources, in addition to the local-only dry-run fixtures.
  - Progress: added and enabled `flow-lifecycle`, a local-only Python recording for existing flow edits. Rust now matches Python status, diff, and dry-run command output for flow config updates, advanced/default step prompt updates, ASR/DTMF updates, function-step create/delete, and no-code exit-condition updates while suppressing unchanged fixed-file resources.
  - Progress: added and enabled `flow-validation`, a local-only Python recording for invalid flow resources. Rust now matches Python validate and push dry-run error output for missing start steps, default-step function references, missing child-step conditions, empty prompts, and bad function-step signatures.
  - Progress: added and enabled `flow-deletion`, a local-only Python recording for no-code condition deletion and whole-flow deletion. Rust now matches Python dry-run command output for `delete_no_code_condition`, no-code step reference updates, and `delete_flow`.
  - Implemented: added and enabled `live-resource-push`, a real Python recording that creates a throwaway branch, pushes a representative flow plus keyphrase boosting resource to Agent Studio, verifies clean status, and deletes the branch. Replay caught and fixed a Rust JSON parity detail: empty `create_flow.no_code_steps` is now omitted to match Python.

- [x] **Validate Python function syntax without requiring Python** (`python-syntax-validation-parity`)
  - Python ADK validates function resources with `compile(code, name, "exec")`; Rust currently only checks expected function signatures for flow function steps and does not syntax-check global/start/end functions.
  - Add Python recording coverage for syntax-invalid global functions, start/end functions, and flow function steps.
  - Implement pure-Rust syntax validation behind an internal wrapper, preferring the Ruff parser lineage if the vendored crate audit is acceptable, and normalize parser diagnostics to the Python ADK error contract.
  - Acceptance: `validate --json` and validation-blocked `push --json --dry-run` report syntax errors for the same resources without depending on a Python interpreter being installed.
  - Implemented: Rust now parses Python function resources with a pure-Rust Ruff-lineage parser wrapper, surfaces syntax failures as Python-compatible read errors for `validate`/validation-blocked `push`, and replays the `python-syntax-validation` fixture covering global, start/end, and flow function-step files. Replay normalizes parser wording while preserving resource names, paths, and JSON error shape.

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
  - [ ] Follow-up: replace Rust's broad `serde_yaml` project-wide YAML formatting with a Python-compatible ruamel-style formatter or narrower resource-aware formatter. Python's `resource_utils.dump_yaml` preserves insertion order, uses literal block scalars for multiline strings, quotes only selected YAML-sensitive keys, and wraps at width 100; Rust's current project-wide formatter can still restyle scalars and wrapping, causing noisy migration diffs.
  - Implemented: projection materialization for topics, entities, handoffs, SMS templates, phrase filters, flow configs, and flow steps now uses ordered serde models mirroring Python `to_yaml_dict()` methods, with exact-text local tests for representative YAML files.
  - Coverage required before marking done: add local-only fixtures that assert exact YAML text for representative topic, flow-step, and broad multi-resource files; include non-alphabetical key order, long wrapped scalars, multiline block scalars, and nested dynamic maps; assert `format --check` reports no affected files on Python-shaped YAML that is already formatted.

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

- [x] **Resolve projection prompt references to local resource names** (`prompt-reference-resolution`)
  - Python rewrites prompt/topic references such as `{{fn:FUNCTION-...}}` and `{{ft:FUNCTION-...}}` to local names like `{{fn:start_verification}}` when materializing resources.
  - Implemented: materialization now resolves `fn`, `ft` (flow-scoped), `attr`, and `vrbl` references in `agent_settings/rules.txt`, topic YAML content, and flow-step prompts to local names during init/pull/switch output.
  - Implemented: push paths now reverse those replacements back to projection IDs for rules/topic/flow-step command generation, preserving no-op round-trips and command payload parity.
  - Acceptance: Rust init/pull/switch materialization matches Python reference names, and push still resolves edited local references back to the correct resource IDs.
