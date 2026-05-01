# Rust ADK Release Gate

Non-interactive compatibility release criteria:

1. `cargo test --workspace` passes.
2. `adk-cli/tests/cli_surface_test.rs` passes for parser/JSON contract checks.
3. `adk-cli/tests/python_parity_test.rs` passes in environments where Python `poly` is installed.
4. No regressions in:
   - exit code behavior for parser errors and command failures,
   - stdout/stderr channel separation for JSON vs human mode,
   - required JSON payload keys for `status`, `diff`, and `push`.
5. Manual smoke matrix (minimum):
   - `poly --version`
   - `poly status --json --path <fixture>`
   - `poly diff --json --path <fixture>`
   - `poly push --json --dry-run --path <fixture>`
