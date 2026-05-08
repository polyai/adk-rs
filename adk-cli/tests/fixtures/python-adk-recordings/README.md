# Python ADK Recordings

This directory stores end-to-end fixtures recorded from the Python ADK against
the real Agent Studio API. The goal is to preserve Python behavior as executable
evidence, then use that evidence to replay the same HTTP traffic while checking
the Rust port.

## Files

- `real-agent-studio.commands.yaml`
  Command-level manifest. It records the Python `poly` invocations, expected exit
  codes, stdout/stderr, and the httpmock cassette file that backs the workflow.
- `real-agent-studio.httpmock.yaml`
  Raw `httpmock` record/playback cassette. It contains the HTTP requests sent by
  Python ADK and the real Agent Studio responses returned through forwarding.
- `real-agent-studio-mutating.commands.yaml`
  Step-level manifest for workflows that mutate a throwaway Agent Studio branch.
  It includes command steps plus explicit `file_edit` steps that a replay test
  must apply to the temp checkout.
- `real-agent-studio-mutating.httpmock.yaml`
  Raw `httpmock` cassette for the mutating branch workflow.

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
  record_real_agent_studio_mutating_branch_workflow_with_python_adk_and_httpmock \
  -- --ignored --nocapture
```

Requirements:

- `poly` is on `PATH`, or `PYTHON_ADK_BIN` points at the Python ADK binary.
- `POLY_ADK_KEY`, `POLY_ADK_KEY_US`, or `POLY_ADK_KEY_US_1` is set.
- The configured project is readable:
  `us-1 / ben-ws / PROJECT-JTQKOKLM` (`Test`).

The read-only workflow covers `init`, local checks, branch/deployment queries,
`pull --force` into a temp directory, and `diff --before main`.

The mutating workflow uses the throwaway branch `adk-rs-recording-mutating`.
It creates the branch, edits `agent_settings/rules.txt`, records `status`,
`diff`, `push --dry-run --output-json-commands`, performs a real branch push,
checks branch-vs-main diff output, then deletes the branch. If regeneration is
interrupted, delete that branch in Agent Studio before rerunning.

## Safety

The recorder replaces the API key value with `<redacted>` before writing the
httpmock cassette. The response bodies still contain real project data, so treat
these fixtures as sensitive. Before committing regenerated fixtures, inspect:

```bash
rg -n "POLY_ADK_KEY|x-api-key|Bearer|secret|token" \
  adk-cli/tests/fixtures/python-adk-recordings
```

## Using For Replay Tests

Future Rust replay tests for command-only manifests should:

1. Load `real-agent-studio.commands.yaml`.
2. Start an `httpmock` playback server from `real-agent-studio.httpmock.yaml`.
3. Substitute `${TMP}` with a temp project directory.
4. Point the Rust CLI at the playback server.
5. Compare Rust command results to the command manifest.

Replay tests for step-level manifests, such as
`real-agent-studio-mutating.commands.yaml`, should also execute each
`file_edit` step before replaying the following command step. The current
`append_text` operation means:

1. Resolve `path` relative to the temp project directory.
2. Append `content`.
3. Continue with the next command step.

Keep newly recorded scenarios narrowly named and source-specific, for example:
`real-agent-studio.commands.yaml` plus `real-agent-studio.httpmock.yaml`.
