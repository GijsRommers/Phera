# Accept the final delimiter in compound transform IDs

- Date: 2026-11-20
- Area: intl transliteration
- Decided by: Design

## Decision

Remove one terminal semicolon from a compound ID. Keep empty intermediate
segments invalid and preserve the supplied ID in creation errors.

## Why

ICU accepts a final semicolon. Laravel's strict transliteration through
portable-ascii uses it in its compound ID for removing marks and converting
to Latin and ASCII.

## Turned down

Dropping every empty segment, which accepts malformed compound IDs.
