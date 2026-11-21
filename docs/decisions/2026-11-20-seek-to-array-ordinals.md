# Seek to array ordinals

- Date: 2026-11-20
- Area: Value layer
- Decided by: Design

## Decision

Translate a live ordinal directly to its slot when the array has no holes.
Use that operation for `array_slice` and array-backed SPL seek. Count live
slots when an array has holes. Object-backed seek still counts only visible
properties.

## Why

Lists and maps both store entries in insertion order. A hole-free slot
already identifies the entry, including when the PHP key is a string or
does not equal its position.

## Turned down

Storing an ordinal index alongside the array. Arrays without holes already
have the required layout.
