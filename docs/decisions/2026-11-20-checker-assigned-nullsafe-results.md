# Link a nullsafe result to its stable local receiver

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

When assigning a nullsafe result to a local variable, remember its
unaliased local receiver. A later proof that the result is non-null
also proves that receiver was available. Reassigning either variable
drops the fact.

## Why

Code often saves a nullable identifier before checking it. A
non-null result from `$entity?->getId()` establishes that a
subsequent use of the same entity is safe.

## Turned down

Keeping the relationship across arbitrary member writes or attaching
it to a receiver exposed through a reference.
