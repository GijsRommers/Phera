# Read packed PDO parameters in place

- Date: 2026-11-20
- Area: Speed and method
- Decided by: Design

## Decision

An emulated statement with positional placeholders can retain a packed PHP
parameter array and visit its values in index order. Keep keyed bindings
for named or sparse input and for native prepares. Share the immutable
parsed SQL between executions of a statement.

## Why

The previous path formatted each position as a string, copied it into a
hash map, and looked it up again during execution. A PHP list already gives
each positional value its SQL index.

## Turned down

Allocating a dense array indexed by arbitrary PHP keys. A sparse key such
as 1000000000 needs a validation error without growing the binding storage.
