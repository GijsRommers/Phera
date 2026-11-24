# Index live JSON ancestors

- Date: 2026-11-21
- Area: Speed and method
- Decided by: Design

## Decision

Search the open containers directly near the root. At depth 32, index
their identities in a set for the rest of that traversal. Update the set
as containers and references open or close.

## Why

The previous traversal updated an ordered set when a stack frame was
temporarily popped and pushed again to visit its next field. On the probe
that encodes a 10,000-level array five times, the work above the separate
setup run fell from 107.0M to 56.3M retired instructions. The set holds
only the currently open ancestors, which also preserves the distinction
between sibling sharing and cycles.

## Turned down

A full ancestor scan grows quadratic on deep acyclic inputs. Updating the
deep index on every work-stack move repeats bookkeeping for containers
that are still open.
