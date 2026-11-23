# Arm PostgreSQL write readiness on backpressure

- Date: 2026-11-21
- Area: PostgreSQL transport
- Decided by: Design

## Decision

On macOS, register PostgreSQL TCP sockets for reads. Try each write on the
nonblocking socket. If it would block, register both interests and let Tokio
await writable readiness. Flush restores read interest on the same socket.
The adapter sits below TLS and uses one initialized receive buffer for safe
reads. Unix-domain sockets and the other platforms keep their transport.

## Why

A delayed loopback reply made kqueue deliver a write-ready event before each
read-ready event with both interests registered. That costs two waits for a
small query. With only read interest, the same exchange took one wait, one
send and one receive. The Linux database trace already measured one wait on
epoll.

The write fallback keeps large statements and COPY asynchronous. Swapping the
reactor registration wakes any stored pending read waker so concurrent readers
re-register without hanging. Tests fill the send buffer to verify pending
write readiness, flush restoration, and a pending reader surviving both swaps.
