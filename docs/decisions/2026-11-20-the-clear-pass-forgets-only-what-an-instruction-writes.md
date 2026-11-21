# The pass that drops unneeded `Clear` instructions forgets only what an instruction may write

- Date: 2026-11-20
- Area: Compiler
- Decided by: Design

## Decision

The optimiser drops `Clear` of a temporary that holds no heap value (`Scalar`: nothing, null, a boolean, a number) or a copy the unit's constant table also owns (`Shared`, from `LoadConst`; it may be an array, so `+` on it is not known to be arithmetic). An instruction it does not model forgets the facts of named locals and of the temporaries it may write: for a path instruction its destination, root, scratch register and bound source; for others what `object_liveness::effect` does not call unrelated. A cleared register holds nothing, and the result of `++`/`--` is never an array.

## Why

Any instruction it did not know wiped every fact, so most `Clear`s stayed: in tight loops they were a fifth of the bytecodes run (`$n++;` left a `Clear` of the unused result in every round).

## Turned down

Dropping the `Clear`s of string temporaries: a string kept alive by a dead temporary makes a later `.=` on the variable copy instead of appending in place.
