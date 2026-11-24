# Cache the native class offset

- Date: 2026-11-20
- Area: Speed and method
- Decided by: Design

## Decision

Keep the native class count in the request class table once the worker's
built-ins have been linked. Put the first-link path out of line.

## Why

Class indices are read during normal dispatch. The first-link path is
rare and adds code to lookups when inlined into dispatch. The cached count
lets a request find PHP-defined classes by index without reopening the
native definitions.

## Turned down

Making each index lookup read the native definitions to get its offset.
