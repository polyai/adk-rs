# Python ADK Upstream PR Audit - 2026-05-28

Scope: merged pull requests in `polyai/adk` from 2026-05-07 through
2026-05-28 inclusive. This audit was generated from GitHub PR metadata, PR
descriptions, changed-file lists, and targeted patches for parity-sensitive
changes.

Source query:

```sh
gh search prs --repo polyai/adk --merged --merged-at ">=2026-05-07"
```

## Action Summary

- Port or fix in Rust: #163, #161, #158, #156, #154, #148, #144, #142, #138,
  #137, #129, #64.
- Verify with a focused parity test or fixture: #159, #136, #135.
- Already covered in Rust: #130, #125, #91.
- No Rust action expected: #155, #153, #149, #147, #146, #145, #141, #140,
  #139, #134, #133, #132, #131.

## Port Or Fix In Rust

| PR | Merged | Upstream change | Rust action |
| --- | --- | --- | --- |
| [#163](https://github.com/polyai/adk/pull/163) | 2026-05-27 | Allows disabled unknown personality adjectives, but filters unknown adjectives out of the update proto. | **Port.** Rust currently forwards all `agent_settings/personality.yaml` adjective keys and has no allowed-adjective filtering. Match Python by only rejecting enabled unknown adjectives and by excluding unknown keys from `update_personality`. |
| [#161](https://github.com/polyai/adk/pull/161) | 2026-05-27 | Adds `poly conversations list`, `poly conversations get`, and `poly conversations get-audio`. | **Port.** Rust has chat conversation URLs, but no conversations command group, public Conversations API client methods, JSON output, or WAV download path. |
| [#156](https://github.com/polyai/adk/pull/156) | 2026-05-27 | Replaces threaded push email parameters with `ADK_COMMAND_USER_OVERRIDE`, used for request headers and command metadata. | **Port.** Rust still exposes `push --email` and threads an actor through push command generation. Align with Python by reading `ADK_COMMAND_USER_OVERRIDE`, setting `X-PolyAI-Email` on relevant requests, using it for `metadata.created_by`, and removing or deprecating `--email`. |
| [#158](https://github.com/polyai/adk/pull/158) | 2026-05-21 | Adds `poly login` for multi-region enterprise Auth0 authentication. | **Port.** Rust has no top-level `login`, Auth0 device/browser flow, region auth mapping, or credential-file save path. This should be implemented with #137 and #142 rather than as a standalone stub. |
| [#154](https://github.com/polyai/adk/pull/154) | 2026-05-19 | Reads the first experimental config entity instead of hardcoding `default`. | **Port.** Rust still hardcodes `experimentalConfig.experimentalConfigs.entities.default` when materializing and comparing experimental features, even though command update IDs can use the first key. Use the actual first config ID consistently. |
| [#129](https://github.com/polyai/adk/pull/129) | 2026-05-15 | Fixes interactive branch merge handling for non-string conflict values. | **Port.** Rust currently stringifies JSON conflict values for display and manual edits always return strings. Match Python by preserving bool/int/float/list values in custom resolutions and by avoiding text merge/edit for object values. |
| [#144](https://github.com/polyai/adk/pull/144) | 2026-05-15 | Sends an empty `ParametersUpdate` for global and transition functions so parameters can be deleted. | **Port.** Rust only emits function parameter updates when it has non-empty parameters, and existing-function updates can preserve remote parameters. Add explicit empty parameter updates for function types that accept parameters. |
| [#137](https://github.com/polyai/adk/pull/137) | 2026-05-15 | Adds `poly start` onboarding: Auth0 signup/auth, API key creation, optional project creation, and initial local setup. | **Port.** Rust has pieces of project creation, but no `start` command, Auth0 handler, PAT creation/list/delete flow, welcome output, or credential-file integration. This is a larger feature port. |
| [#148](https://github.com/polyai/adk/pull/148) | 2026-05-15 | Removes default project-id slugging and skips the project-id prompt for `--region studio`. | **Port.** Rust `project create` still generates a default slug and prompts for project ID for all regions. Match Python by letting Studio generate the ID unless explicitly supplied. |
| [#142](https://github.com/polyai/adk/pull/142) | 2026-05-15 | Saves API keys to `~/.poly/credentials.json`, masks key display, and checks credential availability. | **Port.** Rust currently resolves only environment variables, including per-region variants. Add credential-file resolution before env vars, save with user-only permissions, and mask displayed keys as part of the auth command work. |
| [#138](https://github.com/polyai/adk/pull/138) | 2026-05-14 | Eliminates phantom diffs after `poly pull --force` by changing function header spacing and stripping flow step prompts. | **Port.** Rust still inserts only one newline between a module docstring and the generated function header when imports follow, and it does not trim materialized flow step prompts. Add focused tests because this affects clean Git diffs. |
| [#64](https://github.com/polyai/adk/pull/64) | 2026-05-12 | Adds Python `poly create project`, backed by the Agents API. | **Partially ported.** Rust has the Agents API call and `poly project create`, but Python's CLI shape is `poly create project`. Add the top-level `create project` command or alias, then align its prompts with #148. |

## Verify With Focused Tests

| PR | Merged | Upstream change | Rust action |
| --- | --- | --- | --- |
| [#159](https://github.com/polyai/adk/pull/159) | 2026-05-21 | Adds dynamic tab completion for `branch switch` and makes the Python `review` parser require a subcommand. | **Verify/low priority.** Rust has static shell completion via clap, but not dynamic remote branch-name completion. Rust `review` already returns a non-zero not-implemented message when no subcommand is supplied, so only exact help/UX parity is outstanding. |
| [#136](https://github.com/polyai/adk/pull/136) | 2026-05-12 | Deep-copies `FlowConfig` before Python's temporary dummy start-step swap. | **Verify.** The exact Python mutation bug does not directly map to Rust, but Rust flow creation should be checked for flows whose `start_step` is a function step. Add a parity fixture if missing. |
| [#135](https://github.com/polyai/adk/pull/135) | 2026-05-12 | Normalizes local resources through the project wrapper during pull merge, fixing Function kwargs handling. | **Verify.** Python's missing-kwargs failure is not a Rust class hierarchy issue, but the underlying risk is relevant: pull merge must compare normalized local Function/FlowStep/FunctionStep content. Keep or add focused pull-merge coverage. |

## Already Covered In Rust

| PR | Merged | Upstream change | Rust action |
| --- | --- | --- | --- |
| [#130](https://github.com/polyai/adk/pull/130) | 2026-05-11 | Documents per-region API keys via `POLY_ADK_KEY_{REGION}`. | **Covered.** Rust already resolves `POLY_ADK_KEY_US`, `POLY_ADK_KEY_EUW`, `POLY_ADK_KEY_UK`, plus studio/staging/dev variants before `POLY_ADK_KEY`. |
| [#125](https://github.com/polyai/adk/pull/125) | 2026-05-08 | Adds `deployments show`. | **Covered.** Rust has `deployments show`, prefix lookup, included deployment resolution, JSON output, and human output. Keep replay coverage fresh. |
| [#91](https://github.com/polyai/adk/pull/91) | 2026-05-08 | Adds deployment promote and rollback commands. | **Covered.** Rust has promote/rollback command handling, dry-run payloads, confirmation, active environment aliases, and platform-root `/v1/agents/...` mutation endpoints. |

## No Rust Action Expected

| PR | Merged | Upstream change | Rust action |
| --- | --- | --- | --- |
| [#155](https://github.com/polyai/adk/pull/155) | 2026-05-19 | Updates Python's experimental config schema file. | **No immediate action.** Rust treats `agent_settings/experimental_config.json` as raw JSON and does not embed the Python schema. Revisit only if Rust adds schema validation. |
| [#139](https://github.com/polyai/adk/pull/139) | 2026-05-19 | Speeds up `read_local_resource` by avoiding redundant YAML parsing and caching Python AST function lookup. | **No parity action.** This is mainly Python performance/internal structure. Rust has separate parsing paths. Consider only if profiling shows similar local resource read hot spots. |
| [#153](https://github.com/polyai/adk/pull/153) | 2026-05-18 | Corrects Python `merge_branch` type annotations. | **No action.** Typing-only Python change. Rust already models merge conflicts/errors as structured JSON values. |
| [#147](https://github.com/polyai/adk/pull/147) | 2026-05-15 | Updates Python ADK docs to lead with `poly start` and credential files. | **No immediate action.** Docs-only upstream. Rust docs should be revisited after #137/#142/#158 are ported. |
| [#149](https://github.com/polyai/adk/pull/149) | 2026-05-15 | Removes unused Auth0 constants. | **No action now.** Rust has not ported the Auth0 code yet. Avoid reintroducing removed constants when porting #137/#158. |
| [#146](https://github.com/polyai/adk/pull/146) | 2026-05-15 | Fixes a docs Discord button style and broken icon. | **No action.** Python docs-only. |
| [#145](https://github.com/polyai/adk/pull/145) | 2026-05-15 | Adds a Discord community docs page. | **No action.** Python docs-only. |
| [#141](https://github.com/polyai/adk/pull/141) | 2026-05-14 | Deduplicates and trims Python ADK docs. | **No action.** Docs-only. |
| [#134](https://github.com/polyai/adk/pull/134) | 2026-05-14 | Documents `deployments show`. | **No action.** Docs-only, and Rust already has `deployments show`. |
| [#140](https://github.com/polyai/adk/pull/140) | 2026-05-14 | Documents `poly create project`. | **No docs action yet.** Rust has `poly project create`, not Python's `poly create project` surface. Address under #64 before documenting. |
| [#132](https://github.com/polyai/adk/pull/132) | 2026-05-11 | Documents account/project dicts keyed by ID to avoid duplicate-name collisions. | **No action.** Docs-only in this window. Rust prompts display `name (id)` and carry IDs, so there is no obvious duplicate-name collision from this docs PR. |
| [#133](https://github.com/polyai/adk/pull/133) | 2026-05-11 | Documents deployment promote/rollback commands. | **No action.** Docs-only, and Rust already has `deployments promote` and `deployments rollback`. |
| [#131](https://github.com/polyai/adk/pull/131) | 2026-05-11 | Documents multi-resource YAML file formatting. | **No action.** Docs-only. Continue to treat YAML churn as a watchpoint in Rust. |
