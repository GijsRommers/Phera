# A plain and a guarded copy of the interpreter loop

- Date: 2026-11-19
- Area: Speed and method
- Decided by: Design
- Superseded in part by 2026-11-21 "The guarded loop runs releasing instructions itself and leaves at the next boundary when a destructor waits": the guarded copy no longer sends stores, writes, `foreach` and `instanceof` to the VM

## Decision

`Machine::run` is generic over `const GUARDED: bool`. The
batch runs the plain copy while no live object has a destructor due (a
count on the id pool), otherwise the guarded copy, kept out of line. The
plain copy is `main`'s loop; only the guarded one checks releases in stores,
returns and switches. Creating an object with a destructor leaves the plain
loop. (`crates/vm/src/calls/execution_loop.rs`, `crates/vm/src/fast.rs`)

## Why

Checking for due destructors in every store cost 2 to 8 percent on
the benchmarks, for programs that mostly have no destructors at all. Under
fat LTO the hot loop's code generation is sensitive to any added branch;
two copies keep the common one unchanged.

## Turned down

One loop with a cheap flag test per store (measured 1 to 3
percent); checking only at statement ends (PHP's moment is the release,
inside expressions too).
