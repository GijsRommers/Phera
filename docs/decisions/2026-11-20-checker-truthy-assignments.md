# Carry non-null facts from truthy assignments

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

In `if ($value = read())`, the true branch treats the assigned local as
non-null. A direct truthiness test on a local has the same effect.

## Why

Null cannot reach the true branch. Preserving a nullable return type there
can create false argument and return diagnostics.

## Turned down

Using the assignment's original type unchanged in both branches.
