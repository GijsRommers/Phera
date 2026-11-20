# Keep the platform launch policy

- Date: 2026-11-20
- Area: Speed and method
- Decided by: Design

## Decision

Keep the existing macOS deployment target and system certificate lookup.

## Why

On the startup test Mac, the unmodified CLI spends over 50 million
instructions on `--version`, which does not construct a PHP runtime. A
private build experiment delayed initialization of four Apple frameworks
and saved about 8 million instructions. The remaining launch cost still
exceeded the empty-script budget before any PHP work.

## Turned down

Shipping newer Apple linker flags or changing system certificate lookup
as part of the runtime startup changes.
