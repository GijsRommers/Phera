# Poll PostgreSQL simple queries on the caller

- Date: 2026-11-20
- Area: Speed and method
- Decided by: Design

## Decision

An ordinary `pg_query()` polls its client future directly while the existing
connection task owns the socket. Read response messages as they arrive. After
a cancelled call, drain with an empty query before starting a new metadata
capture if the lease is still in use.

## Why

The synchronous PHP call already waits for its result. A separate lease task
adds another handoff and another completion notification. COPY and
`pg_send_query()` use independent tasks for continued progress.

## Turned down

A separate PostgreSQL client or changing how general PDO lease cancellation works.
