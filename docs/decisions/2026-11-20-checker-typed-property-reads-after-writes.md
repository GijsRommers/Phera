# Respect native property types after assignments

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Keep checking a property assignment against its declared type. On the
continuing path, remove null from the stored value if the native property
type excludes it.

## Why

PHP throws if it tries to store null in such a property. A later read
can use the property's non-null type after a successful write.
