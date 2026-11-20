# Register native extensions on first use

- Date: 2026-11-20
- Area: Speed and method
- Decided by: Design

## Decision

Publish a deferred, process-wide registry for each SAPI. The compiler and
runtime use the same registrations for that SAPI once they need them.

## Why

The empty-script profile separated registry construction from native class
linking. A process needs only the SAPI it runs. Registering both inventories
eagerly and cloning the full registry adds work to unrelated commands.

## Turned down

Eager construction and copying of both SAPI registries at process start.
