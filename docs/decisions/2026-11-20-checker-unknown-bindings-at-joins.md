# Preserve unknown argument bindings across control-flow joins

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

When each incoming path defines a variable or leaves its binding unknown,
keep the joined binding unknown unless all paths define it.

## Why

A dynamic call may initialize a variable through a reference parameter. A
join after that call should preserve uncertainty unless another incoming
path supplies evidence that the variable is missing.

## Turned down

Reporting a possible undefined variable whenever any input binding is
unknown, even when there is no path with evidence of a missing binding.
