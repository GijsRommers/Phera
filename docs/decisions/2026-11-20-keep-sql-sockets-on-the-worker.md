# Keep SQL sockets on the worker

- Date: 2026-11-20
- Area: Server and requests
- Decided by: Design

## Decision

Poll MySQL and PostgreSQL operations on the PHP worker that owns the socket.
Shared PHP handles refer to a worker-local lease. Each operation takes the
driver out of the lease while it awaits I/O and puts it back on completion.
On that path the lease uses neither an async mutex nor a scheduler task.
PostgreSQL's protocol pump, explicitly asynchronous pgsql work, and MySQL's
unbuffered stream use tasks on the same runtime.

## Why

A PDO query previously copied its inputs into a job on a separate Tokio
runtime. That runtime held the connection behind a mutex and returned
driver rows to the PHP worker. Both threads then paid for task scheduling
on a single round trip. The PHP worker already has an async event loop.

## Turned down

Keeping a dedicated SQL I/O runtime. The worker can serve other PHP tasks
while it awaits the network. The former I/O thread setting remains accepted
for existing configurations.
