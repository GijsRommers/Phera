# Write one PCRE capture row directly

- Date: 2026-11-21
- Area: Speed and method
- Decided by: Design

## Decision

Build the capture row directly after a successful `preg_match`.
`preg_match_all` still collects its rows for either ordering.

## Why

A single match can supply at most one row and one mark. On the compiled
route-pattern probe with captures, the work went from 7,279 to 6,968
retired instructions. The count-only probe remained at about 1,620.

## Turned down

Collecting the single row in a temporary vector leaves a heap allocation
between the scanner and the result that the caller receives.
