# Forget array element shapes after iteration by reference

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Widen an array subject before iterating over its values by reference.

## Why

In `foreach ($rows as &$row)`, writing `$row['sale']` can add the key to
`$rows`. The shape recorded before the loop cannot describe later reads.

## Turned down

Inferring complete shapes for element aliases without tracking their
writes across loop iterations and after the loop.
