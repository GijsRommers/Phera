# Intl error state is request-local

- Date: 2026-11-19
- Area: Server and requests
- Decided by: Design

## Decision

Intl error state is request-local. Locale settings use the runtime's INI catalog.

## Why

Concurrent requests see their own errors and locale changes.

## Turned down

Process-global request state or a second INI value store.
