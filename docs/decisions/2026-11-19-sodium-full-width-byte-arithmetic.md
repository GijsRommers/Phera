# Use full-width byte operations for Sodium integer helpers

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Carry over the complete little-endian PHP string in addition and increment. Use subtle to
select the last differing byte for comparison.

## Why

PHP’s strings can exceed the size accepted by a fixed-precision integer constructor.

## Turned down

Fixed-width or variable-time integer operations on arbitrary PHP strings.
