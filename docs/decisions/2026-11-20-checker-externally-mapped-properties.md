# Leave Doctrine-mapped properties out of private-property usage checks

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Skip private-property usage diagnostics for properties with Doctrine ORM
mapping attributes. Resolve the attribute name through PHP imports.

## Why

Doctrine can read and write a mapped property through reflection. A scan
of the declaring class cannot prove its private property is unused.

## Turned down

Skipping all private members on an entity. Unmapped private properties and
methods can still be checked.
