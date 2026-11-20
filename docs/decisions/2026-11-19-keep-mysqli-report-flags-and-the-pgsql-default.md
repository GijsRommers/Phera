# Keep mysqli report flags and the pgsql default connection in private VM request state. Scope their persistent lease keys to the worker

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Keep mysqli report flags and the pgsql default connection in private VM request state. Scope their persistent lease keys to the worker.

## Why

Concurrent requests on a worker need independent PHP state. The worker-scoped keys reuse the SQL registry while preserving PDO's process-wide keys.

## Turned down

PHP-visible globals or thread-local PHP request state.
