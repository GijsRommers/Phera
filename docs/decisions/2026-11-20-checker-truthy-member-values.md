# Narrow guarded member values and shorthand ternaries

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Narrow the value in the true branch of a member guard or shorthand
ternary by removing union members that are always falsy. Keep the
existing invalidation rules for member paths.

## Why

PHP evaluates `$value ?: $default` to `$value` on the true branch.
That value cannot be `null`, `false`, or any other falsy literal.

## Turned down

Removing the entire type of a member that can be both truthy and falsy.
