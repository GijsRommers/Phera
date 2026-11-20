# Keep property documentation separate from implicit initialization

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Use PHPDoc as the property type contract when a property has no
native declaration. Keep track of its implicit null initializer
when deciding whether a null comparison can be proved constant.

## Why

A class can assign a value in its constructor and document that type
without an explicit initializer. Adding null to every later read
creates spurious nullable-argument findings. Declaring a contract
does not make a guard against an uninitialized value redundant.

## Turned down

Widening every property read solely because the type is documented,
or declaring a null guard impossible solely because of its PHPDoc.
