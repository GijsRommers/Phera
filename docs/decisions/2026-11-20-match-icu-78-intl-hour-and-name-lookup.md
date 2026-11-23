# Match ICU 78 hour patterns and character-name lookup

- Date: 2026-11-20
- Area: intl
- Decided by: Design

## Decision

Use ICU 78.2's French, Italian and Ukrainian standalone hour patterns in the
Gregorian skeleton table. Use ASCII case-insensitive exact spelling for
character names and correction aliases, including the canonical hexadecimal
width for algorithmic names. Run the differing oracle cases from PHP 8.5.13.

## Why

Hosted PHP 8.5.13 links ICU 78.2, while the macOS PHP 8.5.12 oracle links ICU
79.1. The older ICU data includes a French hour literal and a narrower
standalone hour in Italian and Ukrainian. The `j`, `J` and `C` skeletons resolve to
the same hour data as explicit 24-hour skeletons in these locales. ICU 79 also
introduced loose character-name matching, so it accepts names that ICU 78
rejects with `U_ILLEGAL_CHAR_FOUND`.

## Turned down

Special-casing the individual calls from the failing matrix or replacing the
rest of the extension's CLDR and Unicode data.
