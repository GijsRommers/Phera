# Site cells are made per function, on first use

- Date: 2026-11-20
- Area: Speed and method
- Decided by: Design

## Decision

A request's per-instruction caches (property sites, call sites linked
through a namespace fallback) are made for a function when it first looks a
site up, one cell per instruction, not for every function of a file when the
file is loaded. (`crates/vm/src/unit_caches.rs`)

## Why

A framework request loads hundreds of files but runs few of their functions;
making and, under snapshot and fork, copying and freeing a cell for every
instruction of every loaded file was a measurable part of each forked
request. Cells keyed by position keep the lookup in the loop as short as it
was.

## Turned down

A table per compiled function that numbers only the instructions with a
site: fewer cells (about 1M fewer instructions per forked Laravel request),
but one more load on every property access in the loop, 1 to 3.5 percent on
the property benchmarks, measured with and without the table in the
function's facts.
