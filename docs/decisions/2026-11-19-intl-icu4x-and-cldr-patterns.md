# Intl links ICU4X 2.4 with full compiled coverage for the linked components

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Design

## Decision

Intl links ICU4X 2.4 with full compiled coverage for the linked components. Retain CLDR
pattern strings for PHP's mutable number and date styles.

## Why

This covers locales beyond the test matrix and preserves PHP's explicit pattern API. Release
size is measured in the intl compatibility notes.

## Turned down

ICU4C and a custom core-locale data subset.
