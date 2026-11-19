# Array separation copies sole-owner references by value

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

When separating a PHP array, copy a reference with a single owner as its
value. Preserve cells with another holder and direct references back to the
original array. Taking a new reference to a sole-owner array element installs
the binding in the writable array.

## Why

PHP checks the reference count when duplicating the table. Dropping an alias
may leave the cell in the original table. Translating that cell during
separation needs no extra scan or changes to plain reads and writes.

## Turned down

Searching arrays when an alias is released.
