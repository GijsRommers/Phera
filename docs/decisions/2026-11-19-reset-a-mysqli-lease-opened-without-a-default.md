# Reset a mysqli lease opened without a default database with COM_CHANGE_USER. Clear PostgreSQL notices after resetting a native lease

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Reset a mysqli lease opened without a default database with COM_CHANGE_USER. Clear PostgreSQL notices after resetting a native lease.

## Why

This restores the original absence of a database and keeps cleanup notices out of the next request.

## Turned down

Reusing the session with the previous request's selected database or notices.
