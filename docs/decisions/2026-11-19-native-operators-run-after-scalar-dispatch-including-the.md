# Native operators run after scalar dispatch, including the fallback for inline literal operands

- Date: 2026-11-19
- Area: Native math and compression
- Decided by: Design

## Decision

Native operators run after scalar dispatch, including the fallback for inline literal operands. Native casts use the value layer's existing conversion paths.

## Why

Math objects can preserve their arithmetic and casts from PHP operators and built-ins without adding extension names to the VM.

## Turned down

Per-extension VM branches and a second scalar conversion path.
