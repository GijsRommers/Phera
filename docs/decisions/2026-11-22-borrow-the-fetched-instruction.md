# Borrow the fetched instruction for the second dispatch

- Date: 2026-11-22
- Area: Speed and method
- Decided by: Design

## Decision

Let `run_simple` return a reference into the active function's immutable
instruction slice. `step` reads its operands there. The reference is tied
to the code slice, so the register window can be released before dispatch.

## Why

On Apple silicon at `4812217f`, tracing one direct call, method call, return
or property read counted 14 native instructions copying the fetched bytecode
across the scalar exit. Each call shape crosses the exit twice per iteration.
The scratch trial saved 34 instructions per function iteration and 35 per
method iteration, including changes in surrounding code generation.

## Structure: was, is

| Structure | Was | Is |
| --- | --- | --- |
| Scalar fallback | Returns an `Instruction` value | Returns a reference into the bytecode slice |
| Second dispatch | Matches the returned value | Matches through the reference |
| Out-of-line handlers and VM exit | Receive an owned instruction | Copy at the boundary that consumes the instruction |

## Turned down

Moving the property-read arm into the second dispatch saved 18 instructions
on the property shape in the separate scratch trial but added 12 to the
loop, 42 to the function shape and 32 to the method shape.
