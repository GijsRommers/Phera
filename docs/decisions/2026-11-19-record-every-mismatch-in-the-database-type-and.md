# Record every mismatch in the database type and fetch matrices

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Record every mismatch in the database type and fetch matrices.

## Why

One failed test can expose many related differences while preserving original values and diagnostics.

## Turned down

Stopping at the first mismatch or dropping failing cases.
