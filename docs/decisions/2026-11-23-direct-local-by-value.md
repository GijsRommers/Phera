# Copy a direct call's single local argument into the callee

- Date: 2026-11-23
- Area: Speed and method
- Decided by: Design

## Decision

For a statically known PHP function with an untyped first parameter passed by value, the compiler marks a single local send immediately before the call. The marker carries the source register and its original argument-area register. The following direct call supplies the callee and final result destination, including any change made by later compiler passes. The machine can copy a defined ordinary value directly into the callee and resume the caller after that call.

## Why

Ledger item 4 counted 36 retired instructions in the caller's move and additional work validating and moving the argument area. It estimated 35–50 saved per function iteration. Item 3 is based on the accepted borrowed-instruction parent at c970da78.

## Measured result

Drop this implementation. The comparator was the private release binary `review/call-path-astra-item2/parent-c970da78` from #200. The candidate was `review/call-path-astra-item3-final/phera-item3` at `56a4b874`. All five shuffled rounds passed their checksums. Values below are the minimum retired instructions and minimum elapsed cycles per iteration, measured separately with the ledger's 1M-to-51M differential in the quiet slot.

| Shape | #200 instructions / cycles | Item 3 instructions / cycles | Change in instructions |
| --- | ---: | ---: | ---: |
| Loop | 145.20 / 21.85 | 149.19 / 21.24 | +3.99 |
| Function | 679.39 / 95.00 | 696.40 / 98.55 | +17.01 |
| Method | 857.39 / 131.18 | 876.40 / 132.71 | +19.01 |
| Property | 565.20 / 74.48 | 573.19 / 74.08 | +8.00 |

The instruction trace confirmed the fast path ran. In the function shape, the parent's move and direct call took 36 + 216 instructions. The candidate's local call took 254, with 161 inside the out-of-line entry helper. The return grew from 247 to 257. Each of the five surrounding scalar opcodes grew by one. The plain loop shows the same additional stack load on the dispatch back edge, four times per iteration. The method call grew by four, its return by ten, and its five surrounding opcodes by one each. The executable traces and counters agree to the nearest instruction on every shape.

The named fixture passed on PHP 8.5.12 and the candidate. Nested calls, argument snapshots, a generator call, a missing required argument and a returning error handler also matched PHP and #200. The separate multiline undefined-argument probe reproduced the parent's existing location difference: PHP points at the argument, and both Phera builds point at the call one line earlier.

The shape failure stopped the trial before the three routes, ten benchmarks, full fixture suite, VM/runtime tests, destructor and take scripts, and both php-src trees. There are no route or suite claims for this revision.

## Structure: was, is

| Structure | Was | Is |
| --- | --- | --- |
| Local operand | Copied into the caller's argument area and then moved again | Copied directly into the new frame on a hit |
| Fallback | A move read the local, then the direct call consumed the area | The local instruction performs the same read at its original position and the following call consumes the area |
| Caller and callee frames | A 16-byte frame with a register window | The same frame and window |

## Turned down

Reading the local later in a multi-argument call could observe a change made by another argument. Taking a local would clear the caller's variable. The marked form needs one local, an adjacent call and a by-value signature. The VM still handles an undefined or referenced argument at the original send, including an error handler that throws before the callee starts.
