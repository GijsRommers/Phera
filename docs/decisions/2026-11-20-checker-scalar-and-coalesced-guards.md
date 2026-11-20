# Narrow values behind scalar and coalesced guards

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

On the true branch of `is_scalar($value)`, restrict the value to
int, float, string, or bool. If a non-null test succeeds on
`$row[$key] ?? null`, also apply the test to the offset.

## Why

A scalar guard makes a later cast well typed. A successful numeric
test cannot have used the null fallback, so it also proves the
original value passed.

## Turned down

Applying the test to the original value when a non-null fallback
could have passed it.
