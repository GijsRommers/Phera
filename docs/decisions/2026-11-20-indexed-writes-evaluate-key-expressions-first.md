# Indexed writes evaluate key expressions first

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

The compiler evaluates expressions in an indexed assignment target before
the right-hand side. It reads a plain compiled variable used as a key after
the right-hand side, at the point of the write.

## Why

An expression such as `$s[$i = 0] = $s[$i]` establishes the key before the
right-hand side runs. In `$a[$i] = ($i = 1)` the write uses the updated
value of the compiled key variable.

## Turned down

Switching the order of all operands or testing whether the destination
currently contains a string.
