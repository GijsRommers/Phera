# Defer the sum type when element inference is unresolved

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Keep the result of an array sum unresolved when the array elements have
the analyzer's internal unknown type. PHP-declared mixed elements still
use the regular builtin signature.

## Why

When loop widening loses the field types of indexed rows, the checker
cannot tell whether a column being summed contains floats.
