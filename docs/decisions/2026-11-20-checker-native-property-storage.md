# Record the coerced value of a native property write

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

When PHP can coerce every alternative to a declared scalar property
type in weak mode, use that type for the stored value and for reading
it after assignment. Check the input to the assignment separately.

## Why

Assigning false to a native string property in weak mode stores the
empty string. A later call to `strlen` receives a string.

## Turned down

Applying scalar coercion to a PHPDoc-only type or assuming that a
potentially invalid conversion always succeeds.
