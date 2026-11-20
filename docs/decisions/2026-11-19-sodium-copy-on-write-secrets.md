# Preserve PHP references when mutating Sodium strings

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Design

## Decision

Mutate by-reference PHP strings with copy-on-write. Erase an exclusively owned buffer when
zeroing a string, then set its reference to null. Preserve raw argument types for Sodium’s
validation.

## Why

Assignments may share a string buffer. PHP applies Sodium’s own reference type checks before
coercion.

## Turned down

Wiping a temporary copy or using a coercing string parameter.
