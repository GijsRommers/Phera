# Handle legacy PostgreSQL escape calls with the request default connection and use the libpq fallback format before a connection exists

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Handle legacy PostgreSQL escape calls with the request default connection and use the libpq fallback format before a connection exists.

## Why

PHP 8.5 still issues a deprecation and escapes strings and bytea in this form.

## Turned down

Assuming a connected server's string settings for a connectionless call.
