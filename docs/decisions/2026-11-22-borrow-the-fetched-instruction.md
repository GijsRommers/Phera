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

## After merging main at `7c91bedb`

The same five-round shape measurement against that main gave:

| Shape | Main instructions | Borrowed instructions | Main cycles | Borrowed cycles |
| --- | ---: | ---: | ---: | ---: |
| Loop | 146.289 | 145.291 | 21.162 | 21.993 |
| Function call | 713.598 | 679.569 | 101.867 | 95.157 |
| Method call | 892.584 | 857.594 | 137.951 | 130.046 |
| Property read | 581.282 | 565.291 | 94.274 | 73.436 |

| Route | Pairs | Main M instructions/request | Borrowed M instructions/request | Change |
| --- | ---: | ---: | ---: | ---: |
| Laravel `/api/hello` | 6 | 25.70 | 25.30 | −1.56% |
| Laravel `/api/posts?page=2` | 6 | 108.58 | 107.05 | −1.41% |
| Symfony `/api/db-yardstick` | 2 | 452.47 | 448.03 | −0.98% |

Each benchmark ran five times per binary. Counts are millions of retired instructions.

| Benchmark | Main low | Borrowed low | Change | Main median | Borrowed median | Change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| fib | 1,795.5 | 1,707.8 | -4.88% | 1,795.6 | 1,708.0 | -4.88% |
| calls | 6,066.9 | 5,833.7 | -3.84% | 6,067.1 | 5,833.9 | -3.84% |
| methods | 19,759.5 | 18,804.7 | -4.83% | 19,919.5 | 18,884.7 | -5.20% |
| properties | 2,281.1 | 2,218.4 | -2.75% | 2,281.2 | 2,218.8 | -2.74% |
| hierarchy | 2,251.6 | 2,184.4 | -2.99% | 2,252.0 | 2,184.5 | -3.00% |
| typed-objects | 5,928.4 | 5,764.9 | -2.76% | 5,929.9 | 5,766.2 | -2.76% |
| objects | 4,048.9 | 3,904.7 | -3.56% | 4,053.2 | 3,916.1 | -3.38% |
| function-objects | 4,039.0 | 3,906.6 | -3.28% | 4,042.5 | 3,913.7 | -3.19% |
| closures | 4,636.2 | 4,509.0 | -2.74% | 4,638.3 | 4,511.3 | -2.74% |
| framework-patterns | 8,686.8 | 8,351.1 | -3.86% | 8,689.7 | 8,351.5 | -3.89% |

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
