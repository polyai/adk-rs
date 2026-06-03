# Python ADK Upstream PR Audit - 2026-06-03

Scope: merged pull requests in `polyai/adk` from 2026-05-07 through
2026-06-03 inclusive. This audit was generated from GitHub PR metadata, PR
descriptions, changed-file lists, and targeted patches for parity-sensitive
changes.

Source query:

```sh
gh search prs --repo polyai/adk --merged --merged-at ">=2026-05-07"
```

## Action Summary

- Port or fix in Rust: #161, #148, #64.
- Verify with a focused parity test or fixture: #159, #136.
- Already covered in Rust: #169, #163, #158, #156, #154, #152, #147, #144, #142, #138, #137, #135, #130, #129, #125, #91.
- No Rust action expected: #155, #153, #149, #146, #145, #141, #140,
  #139, #134, #133, #132, #131, #170, #168, #167, #166.

## Port Or Fix In Rust

| PR | Merged | Upstream change | Rust action |
| --- | --- | --- | --- |
| [#161](https://github.com/polyai/adk/pull/161) | 2026-05-27 | Adds `poly conversations list`, `poly conversations get`, and `poly conversations get-audio`. | **Port.** Rust has chat conversation URLs, but no conversations command group, public Conversations API client methods, JSON output, or WAV download path. |
| [#148](https://github.com/polyai/adk/pull/148) | 2026-05-15 | Removes default project-id slugging and skips the project-id prompt for `--region studio`. | **Partially ported.** Rust now lets Studio generate project IDs unless explicitly supplied. Rust still offers a default slug in the non-Studio project-ID prompt; verify whether that remaining difference should be removed for full Python parity. |
| [#64](https://github.com/polyai/adk/pull/64) | 2026-05-12 | Adds Python `poly create project`, backed by the Agents API. | **Partially ported.** Rust has the Agents API call and `poly project create`, but Python's CLI shape is `poly create project`. Add the top-level `create project` command or alias, then align its prompts with #148. |

## Verify With Focused Tests

| PR | Merged | Upstream change | Rust action |
| --- | --- | --- | --- |
| [#159](https://github.com/polyai/adk/pull/159) | 2026-05-21 | Adds dynamic tab completion for `branch switch` and makes the Python `review` parser require a subcommand. | **Verify/low priority.** Rust has static shell completion via clap, but not dynamic remote branch-name completion. Rust `review` already returns a non-zero not-implemented message when no subcommand is supplied, so only exact help/UX parity is outstanding. |
| [#136](https://github.com/polyai/adk/pull/136) | 2026-05-12 | Deep-copies `FlowConfig` before Python's temporary dummy start-step swap. | **Verify.** The exact Python mutation bug does not directly map to Rust, but Rust flow creation should be checked for flows whose `start_step` is a function step. Add a parity fixture if missing. |

## Already Covered In Rust

| PR | Merged | Upstream change | Rust action |
| --- | --- | --- | --- |
| [#169](https://github.com/polyai/adk/pull/169) | 2026-06-03 | Fixes translation validation when the default language changes locally, and reads `defaultLanguageCode` from projections. | **Covered.** Rust languages/translations read `defaultLanguageCode`, generate default-language updates from local `agent_settings/languages.yaml`, and validate translation coverage against locally configured languages. |
| [#163](https://github.com/polyai/adk/pull/163) | 2026-05-27 | Allows disabled unknown personality adjectives, but filters unknown adjectives out of the update proto. | **Covered.** Rust now matches Python validation by allowing disabled unknown adjectives, rejecting enabled unknown adjectives, and excluding unknown keys from `update_personality`. |
| [#158](https://github.com/polyai/adk/pull/158) | 2026-05-21 | Adds `poly login` for multi-region enterprise Auth0 authentication. | **Covered.** Rust has `poly login`, Auth0 device/browser flow, region auth mapping, PAT setup, and credential-file saving. |
| [#156](https://github.com/polyai/adk/pull/156) | 2026-05-27 | Replaces threaded push email parameters with `ADK_COMMAND_USER_OVERRIDE`, used for request headers and command metadata. | **Covered.** Rust now reads `ADK_COMMAND_USER_OVERRIDE`, uses it for `X-PolyAI-Email` and command metadata authorship, and removed the Python-obsolete `push --email` flag from the CLI surface. |
| [#154](https://github.com/polyai/adk/pull/154) | 2026-05-19 | Reads the first experimental config entity instead of hardcoding `default`. | **Covered.** Rust now uses the actual first experimental config entity ID for materialization, comparison, and update command generation. |
| [#152](https://github.com/polyai/adk/pull/152) | 2026-05-29 | Adds `Translation`, `DefaultLanguage`, and `AdditionalLanguage` resources plus language/translation validation. | **Covered.** Rust materializes, discovers, statuses, validates, and pushes `agent_settings/languages.yaml` and `config/translations.yaml`, including default-language updates, additional-language create/delete, translation create/update/delete, and non-`name` translation-key matching. |
| [#147](https://github.com/polyai/adk/pull/147) | 2026-05-15 | Updates Python ADK docs to lead with `poly start` and credential files. | **Covered.** Rust README setup guidance now leads with `poly start`, `poly login`, and `~/.poly/credentials.json`. |
| [#144](https://github.com/polyai/adk/pull/144) | 2026-05-15 | Sends an empty `ParametersUpdate` for global and transition functions so parameters can be deleted. | **Covered.** Rust now emits explicit empty parameter updates for function types that accept parameters, allowing remote parameters to be deleted. |
| [#142](https://github.com/polyai/adk/pull/142) | 2026-05-15 | Saves API keys to `~/.poly/credentials.json`, masks key display, and checks credential availability. | **Covered.** Rust resolves credential-file keys before environment variables for Python parity, saves with user-only permissions, masks displayed keys, and checks for existing credentials during onboarding. |
| [#138](https://github.com/polyai/adk/pull/138) | 2026-05-14 | Eliminates phantom diffs after `poly pull --force` by changing function header spacing and stripping flow step prompts. | **Covered.** Rust now preserves the Python blank line between module docstrings and imports, strips flow step prompts during materialization and comparison, and has a force-pull clean-status regression test. |
| [#137](https://github.com/polyai/adk/pull/137) | 2026-05-15 | Adds `poly start` onboarding: Auth0 signup/auth, API key creation, optional project creation, and initial local setup. | **Covered.** Rust has `poly start` onboarding with the welcome output, Auth0 sign-in, PAT creation/listing, credential saving, API-key activation wait for project creation, and optional project initialization. |
| [#135](https://github.com/polyai/adk/pull/135) | 2026-05-12 | Normalizes local resources through the project wrapper during pull merge, fixing Function kwargs handling. | **Covered.** The exact Python kwargs bug does not map to Rust, but Rust now has focused coverage for clean pull/status behavior and FunctionStep round-tripping through normalized local content. |
| [#130](https://github.com/polyai/adk/pull/130) | 2026-05-11 | Documents per-region API keys via `POLY_ADK_KEY_{REGION}`. | **Covered.** Rust already resolves `POLY_ADK_KEY_US`, `POLY_ADK_KEY_EUW`, `POLY_ADK_KEY_UK`, plus studio/staging/dev variants before `POLY_ADK_KEY`. |
| [#129](https://github.com/polyai/adk/pull/129) | 2026-05-15 | Fixes interactive branch merge handling for non-string conflict values. | **Covered.** Rust now avoids text merge/manual edit for object values, keeps bool/int/float/list custom resolutions as typed JSON values, and has branch merge coverage for scalar, list, and object conflict paths. |
| [#125](https://github.com/polyai/adk/pull/125) | 2026-05-08 | Adds `deployments show`. | **Covered.** Rust has `deployments show`, prefix lookup, included deployment resolution, JSON output, and human output. Keep replay coverage fresh. |
| [#91](https://github.com/polyai/adk/pull/91) | 2026-05-08 | Adds deployment promote and rollback commands. | **Covered.** Rust has promote/rollback command handling, dry-run payloads, confirmation, active environment aliases, and platform-root `/v1/agents/...` mutation endpoints. |

## No Rust Action Expected

| PR | Merged | Upstream change | Rust action |
| --- | --- | --- | --- |
| [#170](https://github.com/polyai/adk/pull/170) | 2026-06-03 | Documents languages and translations. | **No immediate action.** Rust now has the underlying resources from #152/#169; fold local file layout docs into the next broader Rust docs refresh. |
| [#168](https://github.com/polyai/adk/pull/168) | 2026-05-29 | Documents disabled non-standard personality adjectives. | **No action.** Docs-only, and Rust already covers the underlying #163 behavior. |
| [#167](https://github.com/polyai/adk/pull/167) | 2026-05-29 | Documents `poly conversations list/get/get-audio`. | **No docs action yet.** Rust does not have the conversations command group yet. Address under #161 before documenting Rust usage. |
| [#166](https://github.com/polyai/adk/pull/166) | 2026-05-29 | Clarifies `poly start` is self-serve-only and documents `poly login` for enterprise accounts. | **No immediate action.** Rust already has `poly start` for Studio/self-serve and `poly login --region` for enterprise regions; keep README wording aligned when broader docs are refreshed. |
| [#155](https://github.com/polyai/adk/pull/155) | 2026-05-19 | Updates Python's experimental config schema file. | **No immediate action.** Rust treats `agent_settings/experimental_config.json` as raw JSON and does not embed the Python schema. Revisit only if Rust adds schema validation. |
| [#139](https://github.com/polyai/adk/pull/139) | 2026-05-19 | Speeds up `read_local_resource` by avoiding redundant YAML parsing and caching Python AST function lookup. | **No parity action.** This is mainly Python performance/internal structure. Rust has separate parsing paths. Consider only if profiling shows similar local resource read hot spots. |
| [#153](https://github.com/polyai/adk/pull/153) | 2026-05-18 | Corrects Python `merge_branch` type annotations. | **No action.** Typing-only Python change. Rust already models merge conflicts/errors as structured JSON values. |
| [#149](https://github.com/polyai/adk/pull/149) | 2026-05-15 | Removes unused Auth0 constants. | **No action.** Rust's Auth0 port only includes the active region mappings needed by `poly start` and `poly login`. |
| [#146](https://github.com/polyai/adk/pull/146) | 2026-05-15 | Fixes a docs Discord button style and broken icon. | **No action.** Python docs-only. |
| [#145](https://github.com/polyai/adk/pull/145) | 2026-05-15 | Adds a Discord community docs page. | **No action.** Python docs-only. |
| [#141](https://github.com/polyai/adk/pull/141) | 2026-05-14 | Deduplicates and trims Python ADK docs. | **No action.** Docs-only. |
| [#134](https://github.com/polyai/adk/pull/134) | 2026-05-14 | Documents `deployments show`. | **No action.** Docs-only, and Rust already has `deployments show`. |
| [#140](https://github.com/polyai/adk/pull/140) | 2026-05-14 | Documents `poly create project`. | **No docs action yet.** Rust has `poly project create`, not Python's `poly create project` surface. Address under #64 before documenting. |
| [#132](https://github.com/polyai/adk/pull/132) | 2026-05-11 | Documents account/project dicts keyed by ID to avoid duplicate-name collisions. | **No action.** Docs-only in this window. Rust prompts display `name (id)` and carry IDs, so there is no obvious duplicate-name collision from this docs PR. |
| [#133](https://github.com/polyai/adk/pull/133) | 2026-05-11 | Documents deployment promote/rollback commands. | **No action.** Docs-only, and Rust already has `deployments promote` and `deployments rollback`. |
| [#131](https://github.com/polyai/adk/pull/131) | 2026-05-11 | Documents multi-resource YAML file formatting. | **No action.** Docs-only. Continue to treat YAML churn as a watchpoint in Rust. |
