# Drive stream sockets on the worker

- Date: 2026-11-20
- Area: Streams
- Decided by: Design

## Decision

A stream socket owns a typed Tokio connection or listener on its worker.
TCP, UDP and Unix transports share one socket backing behind the stream
resource. Blocking mode controls whether the PHP operation waits. The
operating system descriptor stays nonblocking. Snapshot creation refuses
live sockets.

## Why

A request waiting for a network operation must leave the worker available
to serve other requests. A snapshot cannot copy a connection or its peer
state into another request. Regular files keep their current I/O paths.

## Turned down

Dispatching each socket read and write to the blocking thread pool.
