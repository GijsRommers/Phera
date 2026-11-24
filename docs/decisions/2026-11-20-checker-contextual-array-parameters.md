# Preserve contextual shapes under a callback's array parameter

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Keep the known array shape of an argument inside a typed inline callback
when it satisfies the callback's native array parameter.

## Why

In `array_map(fn (array $row) => $row['id'], $rows)`, the caller's array
shape can establish the type of the row's id field.
