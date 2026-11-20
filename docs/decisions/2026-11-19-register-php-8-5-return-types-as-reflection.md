# Register PHP 8.5 return types as reflection metadata and distinguish untyped native parameters from explicit `mixed`

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Register PHP 8.5 return types as reflection metadata and distinguish untyped native parameters from explicit `mixed`.

## Why

The stub gives the public declaration independently of how Rust dispatches each call.

## Turned down

Inferring declarations from Rust handlers or reporting every untyped parameter as `mixed`.
