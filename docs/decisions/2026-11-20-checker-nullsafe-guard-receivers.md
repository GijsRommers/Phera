# Refine nullsafe receivers when the chain returns a value

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

When a condition proves a nullsafe chain has a non-null result,
remember the non-null types of its stable receivers on that branch.

## Why

`$request?->hasSession() === true` can succeed only when the request
is available. The guarded code can then call methods on it directly.

## Turned down

Keeping the same fact for a receiver exposed through a reference
or assuming a null result identifies which receiver was null.
