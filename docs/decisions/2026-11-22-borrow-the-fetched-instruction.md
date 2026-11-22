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

The product release build reproduced this with five shuffled rounds.
Each pair ran 1M and 51M source iterations. The difference cancels startup.
The table gives the lowest slope for each counter, in instructions and
cycles per iteration. Every run produced the expected result.

| Shape | Main instructions | Borrowed instructions | Main cycles | Borrowed cycles |
| --- | ---: | ---: | ---: | ---: |
| Loop | 146.375 | 145.328 | 21.522 | 22.134 |
| Function call | 713.762 | 679.742 | 102.792 | 96.457 |
| Method call | 892.685 | 857.807 | 137.878 | 130.018 |
| Property read | 581.422 | 565.364 | 95.298 | 74.204 |

Instructions per warmed request, in millions, against the same main binary:

| Route | Pairs | Main | Borrowed | Change |
| --- | ---: | ---: | ---: | ---: |
| Laravel `/api/hello` | 6 | 25.80 | 25.33 | −1.81% |
| Laravel `/api/hello`, repeat | 6 | 25.72 | 25.30 | −1.62% |
| Laravel `/api/posts?page=2` | 6 | 108.58 | 106.97 | −1.49% |
| Laravel `/api/posts?page=2`, repeat | 6 | 108.60 | 107.02 | −1.46% |
| Symfony `/api/db-yardstick` | 2 | 452.06 | 448.14 | −0.87% |

Hello's paired deltas spanned 0.77 percentage points in the first run
and 0.38 in the repeat, using the gate's rounded counters. Every pair
favoured borrowing. Symfony warmed 300 requests and returned identical
response bytes in both pairs.

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
