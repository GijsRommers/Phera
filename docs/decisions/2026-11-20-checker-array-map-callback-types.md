# Use known array element types in array_map callbacks

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

For a single array argument, use its indexed type to infer the first
parameter of an inline array_map closure. Read that type from existing
variable and member facts.

## Why

The callback receives the array values even though the callback
appears before the array in the call. Type inference for the closure
body needs the element type.

## Turned down

Evaluating the array argument twice or matching several arrays whose
lengths may differ.
