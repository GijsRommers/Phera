# A native call with a PHP-visible frame contributes it when the VM creates the Throwable

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

A native call with a PHP-visible frame contributes it when the VM creates the Throwable. A callback Throwable keeps the trace captured where PHP code threw it.

## Why

Functions and methods report their arguments on fresh body errors, including errors with custom properties. Async handlers retain argument values until the result is known.

## Turned down

Individual exception-trace fixes in extension handlers.
