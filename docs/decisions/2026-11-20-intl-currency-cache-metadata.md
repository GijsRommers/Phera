# Resolve number locale data once per formatter and keep currency rounding metadata with each cached ICU4X formatter

- Date: 2026-11-20
- Area: Safety and dependencies
- Decided by: Design

## Decision

Resolve number locale data once per formatter and keep currency rounding metadata with each
cached ICU4X formatter.

## Why

Release profiling found repeated locale lookup alongside currency rendering. The same
formatter and rounding metadata can serve every call for a cached currency.

## Turned down

Per-call locale and currency data lookup, or an unbounded cache.
