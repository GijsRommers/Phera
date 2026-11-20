# Resolve guarded members on every known receiver arm

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Use each known class arm when resolving a guarded property's type.
The rule also recognizes declared PHPDoc properties.

## Why

Two candidate classes can expose the same nullable property.
A comparison on that property can refine its next read.
