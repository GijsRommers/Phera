# Recognize redundant coalescing to null in bleeding-edge projects

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Honor `unnecessaryNullCoalesce` and the PHPStan bleeding-edge include
for redundant coalescing to a literal null. Require a defined local
and known required array fields, or a known initialized property.

## Why

Coalescing a nullable field to null is redundant when the field must
exist. It can still be useful when an array key or property is absent.

## Turned down

Assuming an optional array key always exists or inspecting an
arbitrary fallback expression for purity.
