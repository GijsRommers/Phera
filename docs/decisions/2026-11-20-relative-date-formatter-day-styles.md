# Use CLDR day names for relative date formatting

- Date: 2026-11-20
- Area: intl formatting
- Decided by: Design

## Decision

Use CLDR day terms and the standard date-time join for PHP's relative date
styles. Compare dates in the formatter's time zone. When CLDR has no term
for the distance, use the absolute pattern of the same style.

## Why

ICU4C uses the same relative terms across all four widths. Relative formatting
falls back to calendar dates even though CLDR also supplies phrases such as
"in 4 days".

## Turned down

The relative-time formatter's numeric fallback and a fixed three-day English
table.
