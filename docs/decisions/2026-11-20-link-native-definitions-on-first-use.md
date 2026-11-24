# Link native definitions on first use

- Date: 2026-11-20
- Area: Speed and method
- Decided by: Design

## Decision

Initialize the worker's native class table on the first class lookup or
declaration. Initialize its native constant values independently, on the
first constant lookup. Each request still owns its own additions and method
lookup state.

## Why

The registry already shares native metadata across threads. PHP values and
linked class definitions can be shared within a worker. Short scripts that
use neither can skip linking layouts and converting constant values.

## Turned down

Eagerly linking both tables when a worker or a CLI VM starts. Sharing linked
classes and PHP values across worker threads.
