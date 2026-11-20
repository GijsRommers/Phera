# Report mysqli method deprecations before validation

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

Use the existing method preparation hook for deprecations such as
`mysqli::ping()`. Preserve the registered PHP parameter list.

## Why

PHP reports these deprecations even for a call rejected by argument validation.

## Turned down

Reporting only after successful validation or loosening the reflected declarations.
