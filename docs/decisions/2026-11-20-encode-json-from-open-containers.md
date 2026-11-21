# Encode JSON from open containers

- Date: 2026-11-20
- Area: Extensions
- Decided by: Design

## Decision

Encode scalars directly. An iterative traversal holds one cursor per open
container, with its top entry inline. Ancestors already held by the
traversal identify recursive arrays, objects, and references. Use the native
synchronous selector for scalars and flat arrays of scalars.

## Why

Only the next member needs encoding. This keeps flat input off the heap
work stack and removes the separate recursion set. Depth is still checked
on closing a container. Object views and serializer calls retain their
place in the traversal across suspension.

## Turned down

Enqueuing a value and punctuation for each member. The open container owns
the source entries and can supply them in order.
