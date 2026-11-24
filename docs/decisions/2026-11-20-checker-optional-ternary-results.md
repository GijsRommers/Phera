# Keep the non-null dependency of an optional ternary result

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Record the receiver dependency of a guarded ternary that returns null
when its input is null and otherwise evaluates a stable expression.

## Why

Testing the result of `$item === null ? null : $item->name()` for
non-null also establishes that the original item exists.
