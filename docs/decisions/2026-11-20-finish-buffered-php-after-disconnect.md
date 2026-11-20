# Finish buffered PHP after a client disconnects

- Date: 2026-11-20
- Area: Server and requests
- Decided by: Design

## Decision

The server previously canceled an unfinished request when Hyper dropped
its response future. Now the owner moves that already-boxed future to a
local task if the client disconnects. The task keeps the request deadline,
continues to count against the worker's connection limit, and joins
graceful shutdown. On a live connection the request stays in its original
future until it finishes or hands off a response.

Buffered PHP output still goes into the capture buffer. The engine opens
a transport when PHP flushes or finishes the response early. If PHP
returns with buffered output after a disconnect, the detached task
discards it.

## Why

In both PHP's development server and PHP-FPM behind unbuffered nginx, a
POST that held output in a buffer reached its final side effect after the
client disconnected. Its connection status stayed normal. When the same
script flushed before the client left, the next failed transport write
ended the script and both the shutdown function and destructor saw
`CONNECTION_ABORTED`.

## Turned down

Polling the socket state on every buffered echo. Starting a local task
for every request would also make the ordinary response pay for
disconnect handling before it needs it.
