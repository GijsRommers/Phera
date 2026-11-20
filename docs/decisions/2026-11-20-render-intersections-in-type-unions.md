# Preserve grouped intersection alternatives in type displays

- Date: 2026-11-20
- Area: Bytecode and type declarations
- Decided by: Design

## Decision

Canonicalize declaration displays when lowering bytecode. An intersection
inside a union is one parenthesized alternative. Signature checks and
lexical type resolution read that grouping from the existing display.

## Why

PHP reports the grouped type in argument, return and inheritance errors.
The alternatives are already representable by the type mask and class-name
list. The display needs no separate copy and the scalar acceptance check
still uses the existing mask.

## Turned down

Reformatting intersection types separately in each diagnostic.
