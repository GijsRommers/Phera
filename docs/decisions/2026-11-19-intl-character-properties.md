# IntlChar uses ICU4X categories and case mapping for common code point functions

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Design

## Decision

IntlChar uses ICU4X categories and case mapping for common code point functions.
Spoofchecker reports unsupported until a compatible implementation is available.

## Why

Unicode classifications should follow the same data release as normalization.

## Turned down

Approximate confusable checks or Rust compiler-dependent Unicode properties.
