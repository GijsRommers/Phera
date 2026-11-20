# Boot at server start, without a request

- Date: 2026-11-19
- Area: Server and requests
- Decided by: Design

## Decision

For a script known to snapshot (an adapter applies, or an
earlier start found the call), each worker runs it before accepting
connections, with no request: `$_GET`, `$_POST`, `$_COOKIE`, `$_FILES` and
`$_REQUEST` empty; `$_SERVER` holds the environment plus `SCRIPT_FILENAME`,
`SCRIPT_NAME`, `PHP_SELF`, `DOCUMENT_ROOT`, `SERVER_SOFTWARE`, `REQUEST_TIME`
and `REQUEST_TIME_FLOAT` (what PHP's command line also sets without a
request, minus `argv`, which a web request has not either); `php://input` is
empty; output is discarded. Every request, the first included, is a fork.
The first start of a plain script cannot know: its first request runs
classically, notices the call, remembers the script in
`$XDG_CACHE_HOME/phera/snapshots/` (else `~/.cache/phera/snapshots/`) and
boots it in the background. A boot that ends without the call makes the
script classic until one of its files changes; a boot that fails is logged
with the worker's number and retried when a request comes after 2 seconds.
(`crates/runtime/src/boot.rs`, `crates/runtime/src/snapshots.rs`)

## Why

The reviewer's change: no user pays for the boot, compiling the
framework happens before the first connection, and no request's values can
leak into the state every request starts from, because there is none.

## Turned down

Snapshotting during the first real request (its superglobals
leak into the snapshot); booting every front controller at start (a plain
script has side effects it should not run without a request).
