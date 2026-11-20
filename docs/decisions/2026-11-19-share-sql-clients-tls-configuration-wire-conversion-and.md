# Share SQL clients, TLS configuration, wire conversion, and the lease registry in `phera-db-drivers`. PDO retains its argument handling and SQLite statement API

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Share SQL clients, TLS configuration, wire conversion, and the lease registry in `phera-db-drivers`. PDO retains its argument handling and SQLite statement API.

## Why

All three PHP APIs can reach the same database code independently. PDO preserves its existing configuration and reset behavior.

## Turned down

Adding database dependencies to the generic `phera-io-pool` crate or making mysqli and pgsql depend on PDO.
