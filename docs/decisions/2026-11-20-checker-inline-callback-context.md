# Infer inline callback parameters from the caller

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Use a documented callable signature or a known native callback
contract to type untyped parameters of inline closures. Explicit
closure parameter types still take precedence.

## Why

A function like `set_error_handler` calls the closure with known
types. Treating these parameters as implicit mixed produces
findings on correctly typed uses in the closure.

## Turned down

Assigning types to callbacks by matching the name of an arbitrary
user function or to closures passed through variables.
