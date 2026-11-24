# Bind template arguments through declared generic bases

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

When a method or function accepts a generic base, project the actual
argument through its declared generic ancestry before inferring templates.

## Why

A concrete request can inherit from a base documented as
`Request<Payment>`. A conditional return tied to that base can resolve to
`Payment` even when the concrete request itself has no template variables.

## Turned down

Using only the generic arguments written on the exact argument class.
