# Check the indexed variant of a conditional class declaration

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

When a file declares one class name in several places, analyse the body whose
declaration appears in the symbol index. Skip the competing class bodies.

## Why

Version-dependent polyfills can declare the same name with different
members. Validating one body's private members against the other variant
produces false missing-member errors.

## Turned down

Merging the member tables of competing class variants. A call could then
appear valid even though no single variant supports it.
