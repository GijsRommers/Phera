# BCMath stores a signed `num-bigint` coefficient with an explicit decimal scale

- Date: 2026-11-19
- Area: Native math and compression
- Decided by: Design

## Decision

BCMath stores a signed `num-bigint` coefficient with an explicit decimal scale.

## Why

Exact integer division truncates toward zero, and the scale preserves trailing zeroes on `BcMath\Number`.

## Turned down

Floating point and a general-purpose decimal crate, which would still require PHP-specific scale and rounding rules.
