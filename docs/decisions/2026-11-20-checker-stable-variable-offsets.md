# Track offsets with stable local keys

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Track accesses like `$rows[$key]` when the root and offset are named.
Discard the associated fact when either variable is written.

## Why

A guard or coalescing assignment can establish an offset's type
without knowing its value. The fact applies to another read with
the same key.

## Turned down

Assuming a key produced by an arbitrary expression stays the same,
or keeping offset facts after reassigning the key.
