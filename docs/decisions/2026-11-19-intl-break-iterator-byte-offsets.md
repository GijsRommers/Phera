# Break iteration stores offsets and rule status from ICU4X boundaries and exposes PHP's native iterator hooks

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Design

## Decision

Break iteration stores offsets and rule status from ICU4X boundaries and exposes PHP's
native iterator hooks.

## Why

PHP positions refer to byte offsets and foreach advances the same break cursor.

## Turned down

Exposing scalar or UTF-16 positions to PHP.
