# Python ADK Recordings

This directory stores end-to-end fixtures recorded from the Python ADK against
the real Agent Studio API. The goal is to preserve Python behavior as executable
evidence, then use that evidence to replay the same HTTP traffic while checking
the Rust port.

## Files

Each scenario has a command manifest and a matching raw `httpmock` cassette:

- `basic-readonly.*`
  Init, local checks, branch/deployment queries, pull, and branch-vs-local diff.
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

## How Recording Works

The ignored integration test
`adk-cli/tests/python_adk_recording_test.rs` owns the recording flow:

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
cargo test -p adk-cli --test python_adk_recording_test -- --ignored --nocapture
```

To regenerate only the mutating branch workflow:

```bash
cargo test -p adk-cli --test python_adk_recording_test \
  record_branch_update_push_with_python_adk_and_httpmock \
  -- --ignored --nocapture
```

To regenerate everything deterministically, run ignored tests sequentially:

```bash
cargo test -p adk-cli --test python_adk_recording_test \
  -- --ignored --nocapture --test-threads=1
```

Requirements:

- `poly` is on `PATH`, or `PYTHON_ADK_BIN` points at the Python ADK binary.
- `POLY_ADK_KEY`, `POLY_ADK_KEY_US`, or `POLY_ADK_KEY_US_1` is set.
- The configured project is readable:
  `us-1 / ben-ws / PROJECT-JTQKOKLM` (`Test`).

The branch scenarios use throwaway branch names prefixed with
`adk-rs-recording-`. If regeneration is interrupted, delete any leftover branch
with that prefix in Agent Studio before rerunning.

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

## Using For Replay Tests

Future Rust replay tests should:

1. Load the scenario's `*.commands.yaml`.
2. Start an `httpmock` playback server from the matching `*.httpmock.yaml`.
3. Substitute `${TMP}` with a temp project directory.
4. Point the Rust CLI at the playback server.
5. Compare Rust command results to the command manifest.

Replay tests should execute each `file_edit` step before replaying the following
command step. Supported operations are:

- `append_text`: append `content` to `path`.
- `write_text`: write `content` to `path`, creating parent directories.
- `replace_text`: replace `target` with `replacement` in `path`.
- `delete_file`: delete `path`.

Keep newly recorded scenarios narrowly named and source-specific, for example:
`dirty-switch.commands.yaml` plus `dirty-switch.httpmock.yaml`.
