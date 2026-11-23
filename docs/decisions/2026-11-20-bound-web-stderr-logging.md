# Bound web stderr logging

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

Send web SAPI `error_log()` and default diagnostic records to a queue with
one dedicated stderr writer. The queue holds at most 4 MiB of log text (and
at most 16,384 records). A record that does not fit is dropped and counted,
and the writer reports the count as one line as soon as it can write again
("phera-server: 12 log records were dropped because the log was not being
read"). On server exit, give the writer a short opportunity to finish.
Explicit file destinations still report their own write results.

## Why

Symfony logs enough per request to fill an unread stderr pipe in a short
series. Awaiting a blocking write ties request completion to the log
reader. A timed-out request can also leave the blocking write behind,
and Tokio waits for that task at shutdown. A detached writer keeps the
request and shutdown deadlines independent of the reader.

## Turned down

Spawning a blocking task per record or holding an unbounded backlog in
memory while the sink cannot accept writes. A queue of 64 records with a
limit of 64 KiB per record (the first version): an ordinary burst of log
lines in one request overflowed it, and the lines vanished without a trace.
