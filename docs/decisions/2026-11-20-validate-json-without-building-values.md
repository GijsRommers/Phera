# Validate JSON without building values

- Date: 2026-11-20
- Area: Extensions
- Decided by: Design

## Decision

Use the decoder's grammar and scanner for validation. Keep container grammar
state, but skip member storage and number or string materialization.

## Why

String escapes, malformed UTF-8, invalid token order, and depth errors use
the same checks as decoding. Validation does not expose values or need to
normalize duplicate keys.

## Turned down

A second parser for validation. It would need to preserve the decoder's
token and error ordering independently.
