# No warning for request fields read before the snapshot

- Date: 2026-11-20
- Area: Server and requests
- Decided by: Design

## Decision

Not built. Because the boot runs without a request, a boot
that reads `$_GET`, `$_COOKIE`, a `REQUEST_*`, `HTTP_*`, `REMOTE_*` or
`QUERY_STRING` key of `$_SERVER`, or `php://input` gets nothing: an empty
array, a missing key (PHP's own "Undefined array key" warning for a direct
read), an empty body. No request's value can reach the snapshot.

## Why

Seeing the silent reads (`$_SERVER['HTTP_HOST'] ?? 'localhost'`)
needs a test on every array read, or superglobals that are objects during
the boot (`is_array()` would then differ between the boot and a request).
The first costs every script; the second changes what the boot computes.
`phera serve` has no `--dev` mode on this base to hang it on either.

## Turned down

A flag on the superglobal arrays checked by every read.
