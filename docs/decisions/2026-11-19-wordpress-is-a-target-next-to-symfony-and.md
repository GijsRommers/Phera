# WordPress is a target next to Symfony and Laravel

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Owner

## Decision

WordPress is a target next to Symfony and Laravel. `mysqli` and the `pg_*` functions are built as thin layers over the database drivers PDO already uses.

## Why

WordPress is the most used PHP application and talks to MySQL only through `mysqli`. One client per database keeps pooling, TLS and type mapping in one place.

## Turned down

A second, separate MySQL client for `mysqli`.
