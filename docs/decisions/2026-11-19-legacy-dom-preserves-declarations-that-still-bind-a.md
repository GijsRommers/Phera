# Legacy DOM preserves declarations that still bind a name after removal, and copies the bindings of imported names into the target arena

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Legacy DOM preserves declarations that still bind a name after removal, and copies the bindings of imported names into the target arena.

## Why

Detached and imported nodes retain their names and namespace URIs after the original parent is freed.

## Turned down

Resolving imported prefixes only during serialization.
