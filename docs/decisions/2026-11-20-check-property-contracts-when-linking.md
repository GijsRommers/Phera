# Check inherited property types when linking

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

Use the same link-time type relation for ordinary and hooked property
overrides. An ordinary visible property has both a read and a write
contract, so the linker checks the relation in both directions.

## Why

The type is part of the contract even when the property has no hooks
or asymmetric visibility. Compatibility can be decided using the class
and type data the linker already has. Property access uses the linked
metadata and adds no checks for inheritance.

## Turned down

Checking inherited declarations again on each property access.
