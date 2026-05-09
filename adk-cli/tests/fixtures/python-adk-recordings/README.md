# Python ADK Recordings

This directory stores end-to-end fixtures recorded from the Python ADK against
the real Agent Studio API. The goal is to preserve Python behavior as executable
evidence, then use that evidence to replay the same HTTP traffic while checking
the Rust port.

## Files

Each scenario has a command manifest and a matching raw `httpmock` cassette:

- `basic-readonly.*`
  Init, local checks, branch/deployment queries, pull, and branch-vs-local diff.
- `branch-merge-main.*`
  Create a branch, edit `agent_settings/rules.txt`, push the branch, merge it
  into main, verify the main checkout, and attempt branch cleanup.
- `branch-update-push.*`
  Create a throwaway branch, edit `agent_settings/rules.txt`, dry-run push
  command generation, perform a real branch push, diff against main, and delete
  the branch.
- `create-delete-dryrun.*`
  Create a throwaway branch, add a local topic, delete a local function, inspect
  status/diff, dry-run push command generation, and delete the branch.
- `dirty-switch.*`
  Create a throwaway branch, dirty the checkout, record switch-without-force
  failure, force switch back to main, and delete the branch.
- `main-push.*`
  Edit `agent_settings/rules.txt` from a main checkout, push, and verify the
  checkout is clean afterward. Python ADK records this as a persistent ADK
  branch rather than a main merge.
- `merge-conflict-resolution.*`
  Merge a small unique topic to main, diverge branch and main on that topic,
  record the unresolved merge conflict, resolve the merge, and attempt branch
  cleanup.
- `pull-conflict.*`
  Use two checkouts of a throwaway branch to push one edit remotely, make a
  conflicting local edit, record pull conflict output, force pull, and delete the
  branch.
- `revert-local.*`
  Edit a local file, record status, revert that file, and record clean status.
- `validation-errors.*`
  Write invalid YAML and record `validate` plus `push --dry-run` error output.

Step-level manifests include command steps plus explicit `file_edit` steps that
a replay test must apply to the temp checkout.

## Rust Test Files

- `record_python_adk_httpmock_fixtures_test.rs`
  Ignored recorder tests. These run the Python ADK against a forwarding
  `httpmock` server, call the real Agent Studio API, and overwrite the
  `*.commands.yaml` plus `*.httpmock.yaml` fixtures.
- `python_adk_recording_fixture_integrity_test.rs`
  Cheap fixture checks. These validate that every scenario has both files, that
  manifests point at the right cassette, and that saved text is portable. They
  do not run the Rust CLI against the recordings.
- `replay_python_adk_httpmock_fixtures_test.rs`
  Cheap Rust replay tests. These start an `httpmock` playback server from each
  saved cassette, run the Rust CLI commands from the matching manifest, apply
  recorded file edits, and compare Rust JSON output and exit codes exactly
  against Python's recorded contract.
- `python_adk_direct_cli_parity_test.rs`
  Direct Python-vs-Rust CLI checks for small local cases. This is separate from
  the httpmock recording/replay workflow.
- `tests/support/mod.rs`
  Shared helper code for CLI spawning, portable recording paths, placeholder
  substitution, scenario names, and httpmock base URL formatting. Add common
  test utilities here instead of re-declaring them in individual test files.

## How Recording Works

The ignored integration test
`adk-cli/tests/record_python_adk_httpmock_fixtures_test.rs` owns the recording
flow:

1. Start a local `httpmock::MockServer`.
2. Configure `server.forward_to("https://api.us.poly.ai", ...)`.
3. Run the Python ADK binary as a child process.
4. Point Python ADK at the mock server with `POLY_ADK_BASE_URL`,
   `POLY_ADK_BASE_URL_US`, and `POLY_ADK_BASE_URL_US_1`.
5. Save the raw httpmock cassette and a smaller command manifest.

This uses httpmock forwarding rather than proxy mode. Forwarding is simpler for
Python `requests` because HTTPS proxy interception needs certificate trust setup,
while forwarding only needs the base URL override.

## Regenerating

Regeneration is intentionally opt-in because it calls the real Agent Studio API
and writes fixture files.

```bash
cargo test -p adk-cli --test record_python_adk_httpmock_fixtures_test -- --ignored --nocapture
```

To regenerate only the mutating branch workflow:

```bash
cargo test -p adk-cli --test record_python_adk_httpmock_fixtures_test \
  record_branch_update_push_with_python_adk_and_httpmock \
  -- --ignored --nocapture
```

To regenerate everything deterministically, run ignored tests sequentially:

```bash
cargo test -p adk-cli --test record_python_adk_httpmock_fixtures_test \
  -- --ignored --nocapture --test-threads=1
```

Requirements:

- `poly` is on `PATH`, or `PYTHON_ADK_BIN` points at the Python ADK binary.
- `POLY_ADK_KEY`, `POLY_ADK_KEY_US`, or `POLY_ADK_KEY_US_1` is set.
- The configured project is readable:
  `us-1 / ben-ws / PROJECT-JTQKOKLM` (`Test`).

The branch scenarios use throwaway branch names prefixed with
`adk-rs-recording-`. If regeneration is interrupted, delete any leftover branch
with that prefix in Agent Studio before rerunning. The `branch-merge-main`,
and `merge-conflict-resolution` scenarios intentionally make permanent changes
to the configured main branch. The `main-push` scenario intentionally persists
the Python ADK's server-side branch created from a main checkout.

## Safety

The recorder replaces the API key value with `<redacted>` before writing the
httpmock cassette. It also normalizes local Python source and virtualenv paths
in command output to `${PYTHON_ADK_ROOT}` and `${PYTHON_ADK_VENV}`. The response
bodies still contain real project data, so treat these fixtures as sensitive.
Before committing regenerated fixtures, inspect:

```bash
rg -n "/home/|/Users/|/tmp/|\.venv|POLY_ADK_KEY|x-api-key|Bearer|secret|token" \
  adk-cli/tests/fixtures/python-adk-recordings
```

## Replay Details

The replay test:

1. Load the scenario's `*.commands.yaml`.
2. Start an `httpmock` playback server from the matching `*.httpmock.yaml`.
3. Substitute `${TMP}` with a temp project directory.
4. Point the Rust CLI at the playback server.
5. Compare Rust command results to the command manifest.

The playback copy is written to a temp directory before use. It relaxes
protobuf command-batch request bodies, because Rust and Python may encode
equivalent protobuf commands with different bytes. It keeps branch-create and
branch-merge JSON requests distinct with partial JSON matchers on stable fields
such as `branchName` and `deploymentMessage`.

JSON-mode commands are strict: replay substitutes only explicit placeholders
such as `${TMP}`, `${COMMAND_ID}`, and `${TIMESTAMP}`, then requires the Rust
JSON payload and process exit code to match the manifest. Human-readable output
can be more flexible, but JSON is treated as a contract.

Replay executes each `file_edit` step before replaying the following command
step. Supported operations are:

- `append_text`: append `content` to `path`.
- `write_text`: write `content` to `path`, creating parent directories.
- `replace_text`: replace `target` with `replacement` in `path`.
- `delete_file`: delete `path`.

For multi-checkout workflows, replay also keeps the first recorded `write_text`
for a path as a seed. This covers httpmock playback's non-stateful handling of
duplicate projection URLs when a later checkout should contain a file that was
created and merged earlier in the same manifest.

Keep newly recorded scenarios narrowly named and source-specific, for example:
`dirty-switch.commands.yaml` plus `dirty-switch.httpmock.yaml`.
