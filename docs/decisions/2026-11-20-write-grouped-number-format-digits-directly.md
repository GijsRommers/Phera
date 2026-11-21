# Write grouped number format digits directly

- Date: 2026-11-20
- Area: Extensions
- Decided by: Design

## Decision

Calculate the result length from the rounded whole and fractional digits.
Write them into one result with the caller's punctuation. Integers yield
decimal bytes from their magnitude and retain their full precision.

## Why

Grouping does not need an intermediate formatted string or a worst-case
separator allowance. The existing decimal rounding keeps deciding which
digits to write.

## Turned down

Using a floating-point conversion for integers or changing the rounding
routine to simplify formatting.
