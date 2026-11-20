# Select method tables by class origin

- Date: 2026-11-20
- Area: Speed and method
- Decided by: Design

## Decision

Resolve a PHP-defined class's method table directly from the class.
Resolve a built-in class's method table from the request's native slots,
which keep request-specific name numbers.

## Why

The method benchmark showed a regression when a call into a PHP class
looked up the size of the lazily linked native table on every dispatch.
The linked class already records which table owns its methods.

## Turned down

Looking up the native class count to select a method table on every call.
