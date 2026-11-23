# UTS #46 uses ICU4X mapping and idna's Punycode, with PHP's separate bidi and joiner options

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Design

## Decision

UTS #46 uses ICU4X mapping and idna's Punycode, with PHP's separate bidi and joiner options.

## Why

PHP exposes per-condition error bits and by-reference output for failed names.

## Turned down

An all-or-nothing URL-host error with mandatory WHATWG restrictions.
