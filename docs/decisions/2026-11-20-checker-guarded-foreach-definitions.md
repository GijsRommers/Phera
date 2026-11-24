# Preserve definitions proved by foreach accumulators

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

With strict rules enabled, an accumulator initialized to `[]` can prove
that a possibly empty foreach ran. After a guard establishes that the
accumulator changed, carry statements and loop variables that were definitely
defined on every continuing path through the loop. Preserve the guard across
plain copies and invalidate it on writes. The strict-rules handling for an
already nonempty iterable still keeps the loop's new variables conditional.

## Why

Changing an initially empty accumulator in a loop requires entering that loop.
The PHPStan strict-rules group reports the overwrite that follows the guard.

## Turned down

Treating all foreach bindings as definitely defined after a loop.
