# Inspect each union receiver while preserving intersection contracts

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

At level 7, check a member against each non-null arm of a union. An
intersection arm can supply a member from any of its bounds. At level 8,
include null unless this is a nullsafe operation.

## Why

`Reader|Writer` must be narrowed to call a method only some values support.
`Reader&Writer` supports both contracts on every value.

## Turned down

Flattening unions and intersections into the same list of classes for
strict member diagnostics.
