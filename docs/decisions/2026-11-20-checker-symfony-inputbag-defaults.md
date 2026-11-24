# Apply the Symfony extension's InputBag default type

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

For a configured Symfony project, remove null from the declared
result of `InputBag::get()` when a known non-null default is supplied.

## Why

The installed PHPStan Symfony extension narrows InputBag results
when the call supplies such a default.

## Turned down

Assuming that an omitted, nullable, or unpacked default excludes null.
