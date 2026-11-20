# The cycle collector counts allocations and uses trial deletion

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Trial deletion over the request's live objects on `Rc`
counts, iterative, with flat edges (`crates/value/src/cycles.rs`). Objects
that hold no heap values and are not attached to anything (leaves) are
skipped and counted at the end. A run is asked for after a number of
object allocations: PHP's adaptive threshold (10,000, growing when a run
frees little), and never before four times as many allocations as there
are live objects. Destructors of garbage run first, then the search is
repeated, then what is still garbage is broken apart. WeakMap values live on
their key objects, so freeing a key frees its value at PHP's moment.
(`crates/vm/src/cycles.rs`, `crates/value/src/attachments.rs`)

## Why

PHP frees cycles with its collector and scripts depend on it (a
million two-object cycles took 700 MB on `main`, 18 MB here, 28 MB in PHP).
Counting allocations instead of PHP's root buffer needs no work on every
decrement. The four-times rule keeps a large live heap without cycles at a
constant cost per allocation.

## Turned down

A root buffer filled on every decrement, as PHP does (a cost
in the hot paths of every release); walking only at the end of the request
(memory grows without bound in long scripts and workers).
