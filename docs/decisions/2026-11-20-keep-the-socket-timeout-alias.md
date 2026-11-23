# Keep the socket timeout alias

- Date: 2026-11-20
- Area: Streams
- Decided by: Design

## Decision

Keep `socket_set_timeout()` with the PHP 8.5 deprecation notice. An async
native entry can opt into the existing deprecation mechanism, so the
notice runs before argument validation or a PHP stream-wrapper callback.

## Why

Applications still use the alias. The callback must be allowed to wait
on the PHP worker without changing the observable order of the notice.

## Turned down

Emitting the notice inside the socket-specific handler after arguments
have been validated.
