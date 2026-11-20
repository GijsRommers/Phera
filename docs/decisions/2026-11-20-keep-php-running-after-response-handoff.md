# Keep PHP running after response handoff

- Date: 2026-11-20
- Area: Server and requests
- Decided by: Design

## Decision

Poll the request on the connection until PHP publishes its response or
returns. Move a request that still has work to a local task on the same
worker. Its connection slot and request deadline still apply and the
worker waits for it during graceful shutdown. If a client disconnects
before PHP produces a response, give PHP the chance to observe it at the
next transport write.

## Why

`fastcgi_finish_request()` can complete the body before running application
cleanup. A disconnect can end PHP at an output boundary and still let it
run shutdown callbacks. In the ordinary case there is a single task for
both the HTTP connection and the script.

PHP-FPM completes the response before an output handler's exception
reaches the caller of `fastcgi_finish_request()`. The body retains the
untransformed bytes from that handler. Phera follows the same order.

## Turned down

Spawning a second task for every request. Canceling PHP immediately when
the connection closes would skip PHP's abort handling and cleanup.
