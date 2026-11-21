# Share SQL socket admission across workers

- Date: 2026-11-20
- Area: Server and requests
- Decided by: Design

## Decision

Count pooled sockets per database from reservation before the handshake to
completed close. Use one process-wide count and FIFO queue for each
database. A worker keeps idle sockets in flat slots with a free list.
A waiting worker can wake another worker to close an idle socket and
release the admission unit. Matching waiters on the same worker can
receive the idle socket after reset. Explicitly unpooled sockets stay
outside the pool limit.

## Why

Previously an I/O runtime held a keyed registry of buckets that all workers
could borrow from. Worker-local ownership avoids that dependency for every
query. Keeping admission shared lets a worker connect even when there are
more workers than admission units.

## Turned down

Fixed quotas per worker and moving live sockets between worker event loops.
