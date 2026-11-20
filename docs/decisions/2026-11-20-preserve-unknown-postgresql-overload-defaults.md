# Preserve unknown PostgreSQL overload defaults

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

Use optional parameters with no declared default for the PHP stub's
`UNKNOWN` defaults in overloaded pgsql functions.

## Why

PHP supports the shorter positional form but reflection reports that no
default value is available. A named call also cannot fill that gap with null.

## Turned down

Publishing an invented null default for the shorter call form.
