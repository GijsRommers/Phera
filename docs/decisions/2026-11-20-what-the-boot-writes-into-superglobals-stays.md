# What the boot writes into the superglobals stays in every request

- Date: 2026-11-20
- Area: Server and requests
- Decided by: Design

## Decision

Each request's superglobals replace the boot's, and then the keys the boot
added, changed or removed in `$_SERVER`, `$_GET`, `$_POST`, `$_COOKIE`,
`$_REQUEST` and `$_FILES` (compared with what the boot run started with) are
changed again in the same way, with the values from the fork's own copy.
(`crates/runtime/src/boot_changes.rs`)

## Why

Under PHP-FPM the boot code runs after the superglobals are set, so what it
writes stays for the rest of the request: Symfony's and Laravel's
environment loaders put their variables into `$_SERVER` this way. Without
this a fork would lose them.

## Turned down

Keeping the boot's `$_SERVER` and adding the request's keys (the request's
values must win where the boot did not write).
