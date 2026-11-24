# Carry branch definitions across complementary readonly enum tests

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

When a readonly property has a two-case enum type, treat comparisons with
its two cases as complementary for definite assignment.

## Why

A variable defined only for write mode can be used in a later write-mode
branch even if the first branch was written as a check for read mode.
The initialized readonly property keeps its value across intervening calls.

## Turned down

Assuming repeated tests on mutable properties still see the same value.
