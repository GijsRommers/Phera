# Use PostgreSQL's built-in large-object functions on the active pgsql lease. Defer file import and export

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

Use PostgreSQL's built-in large-object functions on the active pgsql lease. Defer file import and export.

## Why

Descriptor positions and writes belong to the caller's session. File operations also need local stream and filesystem-policy handling.

## Turned down

A second client for the libpq fast-path protocol or database-server file access.
