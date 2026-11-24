# Keep element context through an unpacked array merge

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Use the element type of an unpacked argument as an input to
`array_merge()`. Carry the merged array's element type into a later
callback, including when inference leaves it unresolved.

## Why

A sequence of typed arrays can be merged without losing the type of
each row. Incomplete inference also needs to remain distinct from a
PHP declaration with missing element types.
