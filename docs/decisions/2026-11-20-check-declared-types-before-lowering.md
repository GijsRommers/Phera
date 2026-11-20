# Validate type declarations before lowering

- Date: 2026-11-20
- Area: Compiler
- Decided by: Design

## Decision

Validate declaration types while the compiler has the written name, its
namespace imports and the declaring class. For compound types, collect
alternatives as ranges into a single list of bounds. Use resolved names to
detect overlapping alternatives and retain the spelling needed in diagnostics.
Relative names in closures and traits can acquire their class later.
Leave those names unresolved when validating their declaration.

## Why

A redundant alternative or illegal bound is a compile error even if the
declaration never runs. The AST already identifies the kind of each clause.
The usual single-name declaration needs no compound type data, and validation
does not add runtime metadata.
Resolve the parent's name only if a compound type mentions it.

## Turned down

Checking the bounds at execution time, when an untaken branch could hide the
error and the declaration's source spelling is no longer available.
