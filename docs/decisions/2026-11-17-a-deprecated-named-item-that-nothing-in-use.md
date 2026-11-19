# A deprecated named item that nothing in use needs is not built and throws `Phera\NotSupportedException` or a compile error

- Date: 2026-11-17
- Area: Scope and compatibility
- Decided by: Owner

## Decision

A deprecated named item that nothing in use needs is not built and throws `Phera\NotSupportedException` or a compile error. Deprecated behaviour of a supported operation stays as in PHP, message included.

## Why

A clear error beats a silently different answer; the list is `crates/source/data/deprecations.tsv`.

## Turned down

Reproducing every legacy corner.
