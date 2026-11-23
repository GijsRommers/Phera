# Retire interrupted SQL sockets

- Date: 2026-11-20
- Area: Safety and dependencies
- Decided by: Design

## Decision

Close a socket if the future using it is dropped before the operation
completes. The pool holds the admission unit until close and opens a
fresh socket for a later borrower. The database rolls back the transaction
on disconnect.

## Why

The previous lease kept a job alive on the separate I/O runtime when
its caller cancelled. This let the same socket survive after the
job finished and the connection reset. With direct awaits, cancellation
can leave the driver halfway through a packet or reply. Closing the socket
keeps that state out of subsequent requests.

## Turned down

Continuing every query in a detached task to preserve the socket.
