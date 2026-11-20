# Normalizer uses ICU4X and the Unicode Character Database's raw compatibility and NFKC case-fold mappings

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Design

## Decision

Normalizer uses ICU4X and the Unicode Character Database's raw compatibility and NFKC
case-fold mappings. Grapheme functions preserve PHP bytes, and searches use collation keys
and a prefix table at cluster boundaries.

## Why

Raw decomposition must preserve its intermediate scalars. A prefix table avoids re-scanning
the haystack for every candidate.

## Turned down

Lowercasing as case folding, recursive decomposition as a raw mapping and unbounded repeated
substring comparison.
