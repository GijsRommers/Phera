# Copy a direct call's single local argument into the callee

- Date: 2026-11-23
- Area: Speed and method
- Decided by: Design

## Decision

For a statically known PHP function with an untyped first parameter passed by value, the compiler marks a single local send immediately before the call. The marker carries the source register and its original argument-area register. The following direct call supplies the callee and final result destination, including any change made by later compiler passes. The machine can copy a defined ordinary value directly into the callee and resume the caller after that call.

## Why

Ledger item 4 counted 36 retired instructions in the caller's move and additional work validating and moving the argument area. It estimated 35–50 saved per function iteration. Item 3 is based on the accepted borrowed-instruction parent at c970da78.

## Structure: was, is

| Structure | Was | Is |
| --- | --- | --- |
| Local operand | Copied into the caller's argument area and then moved again | Copied directly into the new frame on a hit |
| Fallback | A move read the local, then the direct call consumed the area | The local instruction performs the same read at its original position and the following call consumes the area |
| Caller and callee frames | A 16-byte frame with a register window | The same frame and window |

## Turned down

Reading the local later in a multi-argument call could observe a change made by another argument. Taking a local would clear the caller's variable. The marked form needs one local, an adjacent call and a by-value signature. The VM still handles an undefined or referenced argument at the original send, including an error handler that throws before the callee starts.
