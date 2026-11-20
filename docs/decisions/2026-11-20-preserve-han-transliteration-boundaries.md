# Preserve context before Han runs in Any-Latin

- Date: 2026-11-20
- Area: intl transliteration
- Decided by: Design

## Decision

Give the Han transform the Common-script characters immediately before the
run. When a Han run follows a letter directly, preserve ICU's word separator.

## Why

Splitting Any-Latin into script runs removed the context needed for
punctuation and word boundaries. Symfony and Laravel both exposed the
missing separator with Japanese text containing Han and kana.

## Turned down

Inserting spaces between every script change or altering framework vendor
code.
