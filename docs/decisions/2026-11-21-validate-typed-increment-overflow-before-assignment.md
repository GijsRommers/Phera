# Validate typed increment overflow before assignment

- Date: 2026-11-21
- Area: Typed properties
- Decided by: Design

## Decision

When increment or decrement takes a stored integer past its bound,
check whether the selected property and each typed alias holder accept
a float. Diagnose a rejecting declaration before the write. Use the
resolved property or alias from the read so overloaded access need not
run again.

## Why

PHP applies a distinct rule to integer overflow in increment and
decrement. Ordinary arithmetic assignment keeps its usual coercion
rules. The check runs on the overflow path and leaves normal checked
integer increments unchanged.

## Turned down

Changing the generic numeric assignment rules for a float. That
would affect compound assignments and explicit writes.
