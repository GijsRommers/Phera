# Narrow array elements addressed by literal keys

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Track a literal integer or string offset as part of a narrowed place.
Invalidate facts when the root array or its containing member is written.

## Why

A check such as `$items[0] instanceof Part` can establish the type of
the repeated access `$items[0]->value`.

## Turned down

Treating an offset computed from an arbitrary expression as a stable key.
