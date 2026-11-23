# Destructors in a snapshot

- Date: 2026-11-19
- Area: Speed and method
- Decided by: Design

## Decision

A copied object whose class has `__destruct` is destroyed at
the end of its fork's request, as PHP-FPM destroys the boot's objects at the
end of every request. The snapshot's own objects are never destroyed: not
while the worker lives, not when the snapshot is replaced, and not at worker
shutdown. (`crates/value/src/fork/objects.rs`)

## Why

The snapshot is never run, only copied; its destructors would run
side effects (closing connections, flushing logs) that no request asked for.
Each request already ran them on its copies.
