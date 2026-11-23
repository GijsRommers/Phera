# Cache user wrapper status per request

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

The stat family remembers the last successful followed and link queries for
user wrappers. PHP can clear both entries through `clearstatcache()`.
Successful wrapper path mutations clear them too. Plain local file status
continues to come directly from the operating system.

## Why

A repeated status query changes which callbacks PHP runs. Wrapper classes
can observe those calls even when both results would have been the same.

## Turned down

Caching all filesystem metadata or calling the PHP wrapper again on every
repeated query.
