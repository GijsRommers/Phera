# Compare exact intl results for ICU 78 oracle data differences

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

Check the twelve observed ICU 78 differences against exact PHP and Phera
results. Compare every other result directly. Match the complete result,
including diagnostics and any changed reference, and report the ICU version.

## Why

The Ubuntu oracle and Phera's compiled ICU4X bundle produce different UTC
names, Brazilian Portuguese GMT text, and boundaries when extraction begins
inside a Devanagari conjunct. The comparison still detects drift on either side.

## Turned down

Skipping cases, comparing only date prefixes, or allowing arbitrary changes
whenever the oracle links a different ICU version.
