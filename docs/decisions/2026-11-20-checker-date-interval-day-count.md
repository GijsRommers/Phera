# Preserve the known day count of native date differences

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Represent the result of a native date difference as a `DateInterval` with
an integer `days` field. Apply the ordinary property type to other intervals.

## Why

PHP computes a day count when it compares two dates. A duration
constructed from a string can have a false day count.
