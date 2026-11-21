# Count regex matches without capture rows

- Date: 2026-11-20
- Area: Extensions
- Decided by: Design

## Decision

Count matches from the existing scanner when the call has no writable
matches argument. The same scanner validates the subject and offset,
advances after empty matches, and reports errors.

## Why

The caller can observe the count and the error state. Rust capture vectors
and the resulting PHP arrays are unnecessary on this path.

## Turned down

A second match loop for count-only calls. Empty-match retries and error
handling apply to both forms.
