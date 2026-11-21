# Record database sessions during boot

- Date: 2026-11-20
- Area: Database connections and snapshots
- Decided by: Design

## Decision

Record session changes on connections created during a snapshot boot.
The snapshot holds reconnection settings and the boot's statements. Each
fork starts without a socket, borrows from its worker on first use, and
replays the statements before its first operation. Pool admission remains
process-wide, including when a fork opens a pooled socket.

## Why

The original snapshot change looked at SQL on every connection. The VM
knows which connections are opened during boot. Request-created and
replayed connections skip SQL recognition and session-log updates on the
query path. The snapshot keeps the refusals for session state that replay
cannot restore, including transactions, locks, temporary tables, prepared
statements and streaming results.
