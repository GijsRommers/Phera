# Support Laravel's English number rules

- Date: 2026-11-20
- Area: intl number formatting
- Decided by: Design

## Decision

Implement the English CLDR default spellout and numeric ordinal rules and the
spelled-out ordinal selected by Laravel. Keep other languages and the
duration and custom rule styles explicitly unsupported.

## Why

The pinned ICU4X release lacks an RBNF evaluator. The English rules have
bounded numeric decomposition and cover the three Laravel Number helpers in
their default locale. CLDR 49 contains 778,573 bytes of rule text across
101 locale files. The data is small enough to explore a full evaluator.
The English implementation leaves the evaluator's cross-locale rule
substitutions, plural branches and parsing for a later pass.

## Turned down

Falling back to English for other languages, and bundling multilingual rules
before their evaluator, parser and size are verified.
