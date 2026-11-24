# Use generic collection elements for array_map callbacks

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Resolve method result types with the receiver's template arguments when
determining the element type of an array expression. Inspect the input
behind native array filtering and reindexing without running it twice.

## Why

An untyped callback in `array_map($callback, $rows->toArray())` can still
have a known parameter type when the collection declares its elements.
