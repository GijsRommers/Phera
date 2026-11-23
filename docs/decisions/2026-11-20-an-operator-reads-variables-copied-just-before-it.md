# An operator reads variables directly when they were copied just before it

- Date: 2026-11-20
- Area: Speed and method
- Decided by: Design

## Decision

The compiler removes the copies (`Move`) of variables into temporaries that
come directly before a binary operator, with no jump target between, and
lets the operator read the variables. The VM's operator warns about an
unset variable operand, left operand first, as the copy did.
(`crates/compiler/src/passes/direct_operands.rs`, `crates/vm/src/operators.rs`)

## Why

`$i < $n` was three instructions; it is one. Nothing can run between the
copies and the operator, so the values are the same. Copies that something
else separates from the operator (`$a + ($a = 5)`) are kept.

## Turned down

A new register-register compare-and-branch instruction (a new instruction
changes the loop's layout; left for a later step); reading variables late
everywhere as PHP does (PHP prints 10 for `$a = 1; $a + ($a = 5)`, Phera
6; that difference predates this change and is not touched here).
