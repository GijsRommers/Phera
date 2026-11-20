# Preserve PHP’s Sodium introspection order

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

List Sodium functions in PHP order and filter the catalog by actual registration.

## Why

Introspection matches PHP’s order while reflecting the implemented functions.

## Turned down

Advertising catalog entries without an implementation.
