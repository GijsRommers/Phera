# Wait for socket select readiness

Decided by: Design

`socket_select()` uses a zero-timeout kernel select to determine which
sets contain a ready socket. It waits on the worker's reactor and a
timer when no socket is ready. The final select keeps the platform's
semantics for EOF, socket errors and urgent data. A closure that does
not satisfy the requested set stops that reactor watch.

On systems where Tokio exposes urgent-data readiness, exception sets
also use the reactor. On other systems a bounded number of blocking
pool threads wait in kernel select for exception sets. The caller's
timeout or cancellation closes one end of a socket pair and wakes the
wait. This preserves urgent-data support without waking on an interval
or blocking the worker.
