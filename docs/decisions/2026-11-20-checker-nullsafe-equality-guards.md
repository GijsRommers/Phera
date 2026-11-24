# Use non-null comparison peers to refine nullsafe receivers

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

On the equal path of a strict comparison, a non-null local value proves
that the other operand's nullsafe receiver was evaluated.

## Why

The skip path of `$node?->kind() === $kind` can never compare equal if
`$kind` is non-null. The receiver can be used in the equal branch.
