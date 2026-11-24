# Keep skipped paths separate inside a nullsafe chain

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Carry the skipped path through method calls, property reads, and offset
accesses in a nullsafe chain. Join its scope and null result at the
end of the chain. Check the next member against the type from the
path where PHP evaluates it.

## Why

In `$request?->query()->all()`, PHP skips both calls when the request
is null. A nullable result from `query()` on the evaluating path
would still make `all()` unsafe.

## Turned down

Adding null to every intermediate receiver or ignoring all null
results inside a chain.
