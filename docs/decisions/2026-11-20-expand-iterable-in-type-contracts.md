# Compare iterable as a union in type contracts

- Date: 2026-11-20
- Area: Compiler and class linker
- Decided by: Design

## Decision

Keep `iterable` as one type word during declaration validation and one
bytecode mask for value checks. Expand it to `Traversable|array` when
comparing type contracts at link time.

## Why

PHP allows `object|iterable` to be written. Its value set is the same as
`object|array`, since `object` already accepts every `Traversable`.
The subset relation compares those alternatives with the existing rules.

## Turned down

Rejecting the declared `object|iterable` union after expanding the alias,
or testing its members on each value check.
