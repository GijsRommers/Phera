# Shutdown owns the request symbols and generators

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

Publish the live file-scope registers when the request abandons its stack.
Shutdown callbacks borrow the same global bindings that the file and its
includes used. After callbacks, release sole-owned global generators in
reverse symbol order and force-close the remaining live generators in
object-store order. A fatal bailout skips generator finally blocks.

## Why

File-scope registers are execution storage. The request still owns their
values during shutdown, even after a fiber restores a parked caller and
then reports a bailout. Keeping them alive for callbacks also means that
generator closure can no longer rely on the file frame dropping them.

## Turned down

Copying timeout globals as a special case or dropping all globals before
callbacks to trigger generator cleanup through reference counts.
