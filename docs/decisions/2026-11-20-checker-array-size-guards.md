# Keep count guarantees through known array removals

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Remember a lower size bound from a comparison with `count()`. Array
removal lowers the bound by one. A reassignment or affected write clears
it, and a branch join keeps the lowest bound that applies to every path.

## Why

A lower bound of two makes two successive removals return elements.
The declared type of each element still determines whether it can be null.
