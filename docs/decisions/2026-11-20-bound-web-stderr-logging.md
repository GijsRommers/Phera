# Bound web stderr logging

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

Send web SAPI `error_log()` and default diagnostic records to a bounded
queue with one dedicated stderr writer. Drop records when the queue is
full or when a record exceeds 64 KiB. On server exit, give the writer a
short opportunity to finish. Explicit file destinations still report
their own write results.

## Why

Symfony logs enough per request to fill an unread stderr pipe in a short
series. Awaiting a blocking write ties request completion to the log
reader. A timed-out request can also leave the blocking write behind,
and Tokio waits for that task at shutdown. A detached writer keeps the
request and shutdown deadlines independent of the reader.

## Turned down

Spawning a blocking task per record or holding an unbounded backlog in
memory while the sink cannot accept writes.
