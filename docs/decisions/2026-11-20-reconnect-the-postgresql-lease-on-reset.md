# Reconnect the PostgreSQL lease on reset

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

`pg_connection_reset()` replaces the socket within the same lease and updates
the PHP handle's server metadata. The credentials used to reconnect stay in
private connection state.

## Why

PHP reconnects to a new server session. Transaction state, prepared statements,
and queued replies belong to the old session.

## Turned down

Using `DISCARD ALL` on the old socket or acquiring another persistent pool slot.
