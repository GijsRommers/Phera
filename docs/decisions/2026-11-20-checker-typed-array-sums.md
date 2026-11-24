# Infer array sums from known element types

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Use known integer or float array elements to type `array_sum`.
Keep zero as a possible result for an empty array and preserve the
existing representation for integer arithmetic that can overflow.

## Why

PHP returns an integer when an integer sum fits and a float on overflow.
Float elements produce a float, while an empty array returns integer zero.
