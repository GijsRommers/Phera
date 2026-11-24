# Use inherited PHPDoc for untyped method parameters

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

When PHPDoc is enabled, use the prototype's annotation for a corresponding
untyped parameter when the override supplies no annotation. Match the
parameter by position, including after a rename.

## Why

An override can omit an annotation that already describes its input.
Using it at the levels that check PHPDoc lets constant conditions see
the input's documented alternatives before the mixed checks begin.

## Turned down

Replacing a local declaration or annotation with an inherited one.
