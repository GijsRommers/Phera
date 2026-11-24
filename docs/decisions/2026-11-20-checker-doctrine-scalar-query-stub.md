# Use the Doctrine scalar result stub when configured

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

The Doctrine PHPStan include gives `AbstractQuery::getSingleScalarResult()`
the scalar and null return alternatives from its installed stub.

## Why

A scalar query's exact type depends on its projection. Its result can
still be converted to an integer without having to resolve the projection.
