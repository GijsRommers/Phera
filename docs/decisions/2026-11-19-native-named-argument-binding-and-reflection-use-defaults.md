# Native named-argument binding and reflection use defaults and parameter names generated from the current PHP 8.5 stubs

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Native named-argument binding and reflection use defaults and parameter names generated from the current PHP 8.5 stubs. A skipped UNKNOWN parameter raises PHP’s argument count error.

## Why

All native call paths and reflection read the same metadata. Positional handlers keep their own arity and coercion behavior.

## Turned down

Hand-maintained defaults for each call path.
