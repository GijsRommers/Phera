# Share iconv through the extension registry

- Date: 2026-11-20
- Area: Extensions
- Decided by: Design

## Decision

The iconv module provides its byte converter through the native extension
registry. Stream filters take a copy of the function pointer when they are
created. The converter has no access to request state and returns an error
without issuing a PHP diagnostic.

## Why

`iconv()` and `convert.iconv.*` need the same character set conversion. The
mbstring mail functions use the process extension, which depends on streams.
Having the streams crate depend on mbstring would close that dependency cycle.

## Turned down

Maintaining a second converter for stream filters.
