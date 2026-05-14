# Parity Test Strategy

Use two layers:

- **Recordings** prove that Rust matches Python ADK against real/server-shaped HTTP contracts.
- **In-memory matrix tests** provide broad, cheap coverage of resource families, subtypes, operations, and CLI flags.

Prefer adding a matrix row for every parity gap. Add a recording only when the behavior depends on HTTP shape, live server behavior, or Python output that cannot be described locally.

Keep matrix rows small: one fixture, one operation, one expected result. Avoid broad prose tables; the executable rows are the coverage matrix.
