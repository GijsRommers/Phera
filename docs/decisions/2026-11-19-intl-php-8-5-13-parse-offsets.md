# IntlDateFormatter follows PHP 8.5.13 for parse offsets below INT32_MIN

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

IntlDateFormatter follows PHP 8.5.13 for parse offsets below INT32_MIN. The oracle
comparison records its minimum PHP patch release.

## Why

PHP 8.5.12 could truncate some negative offsets into valid offsets instead of rejecting
them.

## Turned down

Preserving integer truncation from the older patch or removing the differing test.
