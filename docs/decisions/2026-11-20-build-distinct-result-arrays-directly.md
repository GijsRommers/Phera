# Build distinct result arrays directly

- Date: 2026-11-20
- Area: Speed and method
- Decided by: Design

## Decision

Check small sets of normalized column keys once per result. Build
associative arrays and stdClass property maps in insertion order when
the keys are distinct. Use normal insertion for collisions, wider
results and fetch modes that also add numeric keys.

## Why

Earlier every row started with an empty PHP array and looked up each
column before inserting it. The column description already gives the
same order and duplicate status for every row.

## Turned down

Checking uniqueness separately for each row or adding a hash table to
the column description.
