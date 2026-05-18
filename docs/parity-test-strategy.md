# Parity Test Strategy

Use two layers:

- **Recordings** prove that Rust matches Python ADK against real/server-shaped HTTP contracts.
- **In-memory matrix tests** provide broad, cheap coverage of resource families, subtypes, operations, and CLI flags.

Prefer adding a matrix row for every parity gap. Add a recording only when the behavior depends on HTTP shape, live server behavior, or Python output that cannot be described locally.

Keep matrix rows small: one fixture, one operation, one expected result. Avoid broad prose tables; the executable rows are the coverage matrix.

For workflows that write user-editable files, semantic equality is not enough. Add local matrix
coverage that asserts idempotence and representative file text:

- A Python-shaped fixture that is already formatted should stay unchanged after `format --check`.
- Pull/materialization tests should assert exact text for a few small representative files, not only
  parse them back into `Value`.
- YAML fixtures must include non-alphabetical key order, long scalars, multiline strings, and nested
  dynamic maps so order/style regressions cannot hide behind command-output parity.
