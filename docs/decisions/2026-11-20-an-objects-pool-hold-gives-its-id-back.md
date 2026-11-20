# An object's pool hold gives its id back itself

- Date: 2026-11-20
- Area: Speed and method
- Decided by: Design

## Decision

`PoolHold`, the last field of `Object`, carries the object's id and whether
the object is an incarnation (it took over another's contents for its
destructor), and gives the id back to the request's pool when it is dropped.
`Object`'s `Drop` only hands over an object whose destructor is due. The
"releasing" stack of the pool, and the `incarnated` field of `Object`, are
gone. (`crates/value/src/object.rs`, `crates/value/src/destruction.rs`)

## Why

Freeing an object pushed its id on a stack in `Object::drop` and popped it
in `PoolHold::drop`: two borrows of the pool per object, added with the
destructors work. Rust drops fields in declaration order, so the hold's
drop already comes after everything the object held is freed, which is the
order PHP reuses ids in; the stack added nothing. With the hold carrying the
id, and the tracking and the constructed-object call inlined again, creating
and freeing an object costs 839 machine instructions (micro benchmark),
against 872 on batch 14 and 779 before the destructors work.

## Turned down

Leaving objects without a destructor out of the request's object list (the
end of a request needs it to break their cycles); skipping the allocation
count for them (automatic cycle collection counts every object).
