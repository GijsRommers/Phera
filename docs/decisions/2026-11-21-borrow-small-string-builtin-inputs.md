# Borrow small string built-in inputs

- Date: 2026-11-21
- Area: Speed and method
- Decided by: Design

## Decision

Trim a single character without building a character mask and share the
original bytes when a trim leaves them unchanged. In the array form of
strtr, use the already typed arguments and share string keys while
ordering the replacement patterns.

## Why

The application frequently trims namespace separators and translates
short names. These calls can reuse the strings they already own. The
general character-mask parser, conversion order and longest-match
replacement rules still handle the full PHP argument range.

## Turned down

Changes to native argument binding and result delivery belong to the
shared call protocol.
