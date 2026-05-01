# ADK Python -> Rust CLI Compatibility Matrix

This matrix defines the non-interactive compatibility contract for `poly`/`adk`.

## Global

- Entrypoints: `poly`, `adk`
- Root flag: `-v, --version`
- Error channels:
  - human mode: user-facing errors on stderr
  - json mode: single JSON payload on stdout
- Baseline exit behavior:
  - parser errors: `2`
  - keyboard interrupt: `130`
  - command failures: `1` (except known Python legacy cases where `success: false` may still exit `0`)

## Command/Flag Surface

### `docs`
- Positional: `documents...` (choices-based)
- Flags: `--all`, `--output|--write|-o`, `--verbose`

### `init`
- Flags: `--base-path`, `--region`, `--account_id`, `--project_id`, `--format`
- Hidden: `--from-projection`, `--output-json-projection`
- Common: `--json`, `--debug`, `--verbose`

### `pull`
- Flags: `--path`, `--force|-f`, `--format`
- Hidden: `--from-projection`, `--output-json-projection`
- Common: `--json`, `--debug`, `--verbose`

### `push`
- Flags: `--path`, `--force|-f`, `--skip-validation`, `--dry-run`, `--format`, `--email`
- Hidden: `--from-projection`, `--output-json-commands`
- Common: `--json`, `--debug`, `--verbose`

### `status`
- Flags: `--path`, `--json`, `--verbose`

### `revert`
- Flags: `--path`, `--json`, `--verbose`
- Positional: `files...`

### `diff`
- Flags: `--path`, `--files...`, `--before`, `--after`, `--json`, `--verbose`
- Positional: `hash?`

### `review`
- Parent flags: `--path`, `--json`, `--verbose`
- Subcommand is intentionally optional for parser compatibility.
- `create`:
  - Positional: `hash?`
  - Flags: `--before`, `--after`, `--files...`, `--json`, `--verbose`
- `list`: `--json`
- `delete`: `--id`, `--json`

### `branch`
- Subcommand is required.
- Common flags: `--path`, `--json`, `--debug`, `--verbose`
- `list`: no extra flags
- `create`:
  - Positional: `branch_name?`
  - Flags: `--env|--environment`, `--force|-f`
- `switch`:
  - Positional: `branch_name?`
  - Flags: `--format`, `--force|-f`
  - Hidden/internal: `--from-projection`, `--output-json-projection`
- `current`: no extra flags
- `delete`: positional `branch_name?`
- `merge`: positional `message?`, `--interactive|-i`, `--resolutions`

### `format`
- Flags: `--path`, `--files...`, `--check`, `--ty`, `--json`, `--verbose`

### `validate`
- Flags: `--path`, `--json`, `--verbose`

### `chat`
- Flags:
  - `--path`
  - `--environment|-e`
  - `--variant`
  - `--lang`
  - `--input-lang`
  - `--output-lang`
  - `--channel`
  - `--functions`
  - `--flows`
  - `--state`
  - `--metadata`
  - `--push`
  - `--message|-m` (repeatable)
  - `--input-file`
  - `--conversation-id|--conv-id`
  - plus `--json`, `--debug`, `--verbose`

### `completion`
- Positional: `shell` in `{bash|zsh|fish}`
- Must support scripts for both command names (`poly`, `adk`)

### `deployments`
- Subcommand required.
- `list` flags:
  - `--path`
  - `--env|-e`
  - `--limit`
  - `--offset`
  - `--hash`
  - `--details`
  - `--json`
  - `--verbose`

## Behavior Notes To Preserve

- `--verbose`/`--json`/`--debug` are not root-global in Python; they are parser-parent flags on relevant commands.
- Some commands in Python emit `{ "success": false, ... }` while still returning `0`; preserve this where currently observable in non-interactive behavior.
- JSON mode exception behavior wraps error + traceback and exits `1`.
