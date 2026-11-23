# Use PHP 8.5's PostgreSQL stub as the alias list. Share parameter declarations and query handling with the canonical functions

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Use PHP 8.5's PostgreSQL stub as the alias list. Share parameter declarations and query handling with the canonical functions.

## Why

Retains the spelling and deprecation diagnostics applications see when they call an alias.

## Turned down

Registering names absent from the stub or giving aliases separate SQL implementations.
