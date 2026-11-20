# Build shallow array combinations in one pass

- Date: 2026-11-20
- Area: Extensions
- Decided by: Design

## Decision

Merge shallow arrays by appending integer entries and writing string entries
directly into one result. Shallow replacement copies the first array and
writes later entries over it. Only recursive combinations allocate frames.

## Why

A shallow overwrite does not need the destination's old value. Integer
entries use the result's existing append operation, and string overwrites
keep their insertion position.

## Turned down

Returning a source array directly from a shallow merge. Sources can carry
internal pointer and append state that differs from a newly merged result.
