# Let pgsql polls advance the socket

- Date: 2026-11-20
- Area: Server and requests
- Decided by: Design

## Decision

When a pgsql query is pending, give the worker's event loop a turn during
`pg_connection_busy()` and `pg_consume_input()`. Both keep their PHP
signatures and return before the statement has to finish.

## Why

PHP programs may poll `pg_connection_busy()` in a tight loop after
`pg_send_query()`. Previously PostgreSQL's I/O runtime could make progress
while PHP polled. On the same worker, the socket and the poller need a
chance to run in turn.

## Turned down

Waiting for the entire statement inside a poll or moving pgsql's socket
back to another runtime.
