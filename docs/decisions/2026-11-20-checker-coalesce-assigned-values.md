# Keep the joined value of a coalescing assignment

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Record the value at a stable target after `??=`. Join the original
value without null with the assigned value on paths that continue.

## Why

After `$this->cache ??= new Cache()`, a read of the same property
gets a cache regardless of which branch ran.

## Turned down

Discarding the updated type whenever the two branches have different
source locations or choosing only the fallback's type.
