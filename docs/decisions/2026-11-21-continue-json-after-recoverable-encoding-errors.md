# Continue JSON after recoverable encoding errors

- Date: 2026-11-21
- Area: Scope and compatibility
- Decided by: Design

## Decision

Keep the JSON error code separate from the decision to stop visiting values.
A malformed key records an error and its value still runs. A non-finite
number records an error and writes its zero placeholder. With partial
output enabled, traversal also continues after errors that would otherwise
stop it.

## Why

Real PHP reports `JSON_ERROR_INF_OR_NAN` for `["\xff" => NAN]` and
`JSON_ERROR_UTF8` for `["number" => NAN, "\xff" => 1]`. A JSON serializer
callback can still run after either recoverable error. The recorded error
controls the final return value and `JSON_THROW_ON_ERROR`.

## Turned down

Stopping on the first recorded error also stopped PHP-visible callbacks
and prevented later errors from taking precedence.
