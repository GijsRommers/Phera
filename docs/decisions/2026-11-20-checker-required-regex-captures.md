# Preserve required captures in regex match rows

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Use a required field for a numbered capture when a literal pattern
proves it participates in every match. Keep the nullable fallback for
other captures and patterns.

## Why

`PREG_UNMATCHED_AS_NULL` makes an optional capture nullable. A group
outside optional or unsupported constructs still yields a string.
