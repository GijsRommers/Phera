# Bundle Unicode character names and ICU property aliases

- Date: 2026-11-20
- Area: intl character data
- Decided by: Design

## Decision

Bundle Unicode 18 explicit names, correction aliases and range prefixes.
Compute Hangul syllable names from the Jamo short names. Use ICU's public
numeric property IDs to identify property aliases.

## Why

Unicode's data provides the names used by the PHP Unicode API. IntlChar's
property numbers and short, long and alternate aliases must agree with ICU4C.
The name data increased a standalone optimized arm64 executable by 1,155,840
bytes before adding the 58,003-byte property alias table and lookup code.

## Turned down

Expanding algorithmic names into one row per code point or obtaining names
through an ICU4C runtime dependency.
