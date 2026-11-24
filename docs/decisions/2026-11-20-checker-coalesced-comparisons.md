# Narrow a coalesced value when the fallback cannot pass a comparison

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

For ordered comparisons with known integer operands, check what the
comparison would do if it used the fallback. If that contradicts the
current branch, narrow the original value to non-null.

## Why

A branch guarded by `($id ?? 0) > 0` can only run when `$id` exists.

## Turned down

Narrowing when the fallback could satisfy the branch or guessing
how PHP compares arbitrary objects and strings.
