# Preserve an arrow result's array shape

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Use the known array shape of an arrow expression when it satisfies the
arrow's native `array` return type.

## Why

A callback such as `fn (int $id): array => ['id' => $id]` creates a row
with an integer field. A later `array_map` or `array_column` can use it.
