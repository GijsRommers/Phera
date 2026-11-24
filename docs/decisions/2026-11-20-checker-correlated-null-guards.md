# Remember when two local variables cannot both be null

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

On the false path of two strict null tests joined by AND, record
that the two local variables cannot both be null. A later strict
null test can use that fact. Reassigning either variable drops it.

## Why

The early return in `if ($a === null && $b === null) { return; }`
makes `$b` safe to use after a subsequent check that `$a` is null.
Each variable considered separately still looks nullable.

## Turned down

Keeping relationships between mutable member paths or tracking
arbitrary boolean formulas across branches.
