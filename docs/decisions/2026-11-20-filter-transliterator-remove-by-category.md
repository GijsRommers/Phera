# Filter Transliterator Remove by Unicode general category

- Date: 2026-11-20
- Area: intl transliteration
- Decided by: Design

## Decision

Resolve general-category aliases in filtered Remove identifiers with ICU4X.
Apply the filter when executing that transform in the compound identifier.

## Why

Accent stripping commonly decomposes text, removes nonspacing marks and
recomposes it. ICU4X already provides the category names and character data.

## Turned down

Hard-coding the spelling of the nonspacing-mark filter or introducing a
general UnicodeSet interpreter for this transform.
