# Defer allocation-free type and INI strings

- Date: 2026-11-20
- Area: Value layer
- Decided by: Design

## Decision

Leave the representation of values returned by `gettype()` and `ini_get()`
to the wider string layout design.

## Why

`PhpString` currently stores an `Rc<Vec<u8>>`. Both the bytes and their
reference-counted owner allocate when a borrowed label or default becomes
a PHP value. Inline or static strings need to integrate with value size,
mutation, and string ownership across the runtime.

## Turned down

Retaining copies of these strings solely to avoid allocations on later
calls.
