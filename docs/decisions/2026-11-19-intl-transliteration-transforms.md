# Use compiled transliteration transforms, Unicode script runs, and simple ICU4X casing and normalization for compound IDs

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Use compiled transliteration transforms, Unicode script runs, and simple ICU4X casing and
normalization for compound IDs. Offer BGN only for supported language-specific IDs.

## Why

Slug generators compose these operations and depend on the generic BGN lookup failing so
they can fall back to Any-Latin.

## Turned down

Transliteration through host programs, inventing a generic BGN transform or approximating
spelling with a hand-written alphabet map.
