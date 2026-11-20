# Use a bounded MessageFormat 1 parser over the intl number, date and ICU4X plural APIs

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Use a bounded MessageFormat 1 parser over the intl number, date and ICU4X plural APIs. Use
the same locale-aware parsers to recover indexed numbers and dates from a message.

## Why

ICU4X has no MessageFormat 1 implementation and PHP accepts nested named branches and typed
indexed fields.

## Turned down

Flat string substitution, ASCII-only number parsing and linkage to ICU4C.
