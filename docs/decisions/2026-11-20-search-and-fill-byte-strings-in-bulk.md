# Search and fill byte strings in bulk

- Date: 2026-11-20
- Area: Extensions
- Decided by: Design

## Decision

Use the workspace's byte-search library for case-sensitive searches
after checking for a direct hit at the search boundary. Fill a one-byte
repeat by resizing the reserved output. Use four machine
words for byte membership when both the span and its character set justify
building the bitset.

## Why

These operations all work on arbitrary bytes. Bulk search and fill cut the
work for long inputs, and a byte has exactly 256 possible values. Small
span inputs can still use the direct membership check.

## Turned down

Preparing a reusable search or membership table for user strings. The
operations only need transient data from the current call.
