# Generate date patterns from CLDR skeletons

- Date: 2026-11-20
- Area: intl formatting
- Decided by: Design

## Decision

Use Gregorian CLDR available formats and the existing date style tables to
select a localized pattern for a requested set of fields. Resolve the
preferred hour cycle from the locale and combine date and time when a single
candidate covers neither combination.

## Why

PHP accepts run-time UTS #35 skeleton strings. ICU4X's typed field sets do
not expose ICU4C's general pattern generator. The filtered CLDR table adds
376,971 bytes to the source data.

## Turned down

Returning a pattern from a fixed list of English skeletons or using ICU4C at
run time.
