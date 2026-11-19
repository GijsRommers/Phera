# Release stored class values at request teardown

- Date: 2026-11-19
- Area: Server and requests
- Decided by: Codex

## Decision

Clear static property cells and evaluated class constant values when class definitions are dropped.

## Why

Static singletons and enum aliases can hold objects that own the same class. Dropping the object property table alone cannot break that edge.

## Turned down

Owning mutable class values beyond the lifetime of a request, or running PHP destructors again during Rust drop.
