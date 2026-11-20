# Collator formats locale-sensitive order and keys with ICU4X, and reuses the bounded Zend sort port for PHP arrays

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Collator formats locale-sensitive order and keys with ICU4X, and reuses the bounded Zend
sort port for PHP arrays. The sort-key API uses empty keys for nonstrings and the raw Zend
tie order.

## Why

Mixed PHP values can produce inconsistent comparisons, and the two PHP sort entry points
resolve ties differently.

## Turned down

Rust's generic sort for PHP mixed values and treating ICU sort keys as portable byte
strings.
