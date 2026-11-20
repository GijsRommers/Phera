# Copy stream registrations into forks

- Date: 2026-11-20
- Area: Server and requests
- Decided by: Design

## Decision

Each fork gets its own wrapper and filter registration tables, including
changes the boot made to the built-in wrapper table. A snapshot that holds
an open user wrapper or an attached filter is refused and names the handle
or filter. Filesystem observations and wrapper recursion guards start fresh.

## Why

Applications can configure their wrappers and filters once during boot.
Open handles have cursors and callbacks, and filters can have pending input
or output. Their state belongs to the request that opened them.

## Turned down

Reopening a boot's streams or replaying its filter callbacks in every fork.
