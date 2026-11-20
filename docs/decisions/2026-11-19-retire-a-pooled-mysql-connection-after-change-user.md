# Retire a pooled MySQL connection after `change_user()`

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Retire a pooled MySQL connection after `change_user()`.

## Why

Its pool key records the original identity.

## Turned down

Lending the connection again under its old credentials.
