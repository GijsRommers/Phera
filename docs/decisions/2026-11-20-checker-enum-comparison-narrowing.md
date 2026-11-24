# Use enum case comparisons to prove a value exists

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

When strict equality with a known enum case succeeds, narrow the
other operand to that enum. Apply existing nullsafe receiver
refinements to the operand as well.

## Why

An expression equal to `Mode::Active` cannot be null. If it came from
a nullsafe property read, the receiver that supplied the value exists.

## Turned down

Inferring the value of an unresolved class constant or narrowing
on an inequality that null could satisfy.
