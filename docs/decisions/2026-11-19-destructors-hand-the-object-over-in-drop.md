# Destructors: hand the object over in `Drop`

- Date: 2026-11-19
- Area: Speed and method
- Decided by: Design

## Decision

An object whose class declares `__destruct` is created with
its destruction `Due`. When its last `Rc` goes, `Drop for Object` moves its
contents (class, properties, native state, attachments, its hold on its id)
into a new `Rc<Object>` marked `Done` and queues that on the request's id
pool. The interpreter runs the queue at instruction boundaries, before the
next instruction. (`crates/value/src/destruction.rs`)

## Why

PHP runs a destructor at the moment the count reaches zero. A Rust
`Drop` cannot run PHP code (no interpreter, no `await`), and by then the
`Rc` is gone, so the object cannot be `$this`. Rebuilding it from moved
fields needs no `unsafe`, loses no destructor and runs none twice.

## Turned down

Reviving the dropped allocation (needs `unsafe`); scanning for
dead objects after each statement (cost in proportion to the heap, and the
moment is wrong); a tracing collector instead of reference counts (changes
every value in the VM, and PHP's moment is reference counting's moment).
