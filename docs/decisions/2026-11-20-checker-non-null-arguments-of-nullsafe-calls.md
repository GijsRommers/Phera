# Narrow nullsafe receivers while their arguments run

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Keep a receiver's non-null type on the path that evaluates a nullsafe
member access. Join it with the skipped path when the expression ends.

## Why

In `$node?->setName($node->name())`, the inner call runs only when the
receiver is available. Later statements can still see a null receiver.
