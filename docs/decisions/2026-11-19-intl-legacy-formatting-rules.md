# Intl preserves ICU4C's legacy grouping and half-even currency rounding, Arabic GMT offset signs, literal fractional-second separators, and the Gregorian calendar's Julian parsing cutover

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Intl preserves ICU4C's legacy grouping and half-even currency rounding, Arabic GMT offset
signs, literal fractional-second separators, and the Gregorian calendar's Julian parsing
cutover.

## Why

These outcomes differ between the APIs even with the same CLDR release.

## Turned down

Accepting deterministic differences as locale data drift.
