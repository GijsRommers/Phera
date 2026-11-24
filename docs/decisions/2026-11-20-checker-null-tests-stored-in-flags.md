# Preserve non-null facts stored in local flags

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Attach a non-null fact to the applicable outcome of a local boolean
assignment. Discard the fact if the flag or tested variable changes.

## Why

In `$ready = $node !== null; if ($ready) $node->run();`, the flag still
describes the receiver's state. A boolean expression can carry the facts
that hold on its true and false paths.
