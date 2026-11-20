# SimpleXML iteration follows the live arena links

- Date: 2026-11-20
- Area: Compatibility
- Decided by: Design

## Decision

A SimpleXML iterator pins its current node and finds the next matching sibling in the shared
arena. A view and any yielded object retain their document even after the original root object
is released.

## Why

PHP sees children appended during traversal and skips siblings removed before it reaches them.
A pinned current node remains usable after removal, with no successor once detached.

## Turned down

Copying the selection's node indices into a list at rewind time.
