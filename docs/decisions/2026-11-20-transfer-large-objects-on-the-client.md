# Transfer PostgreSQL large objects on the client

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

Transfer local files with Tokio and PostgreSQL large-object descriptors on the
caller's connection. Use binary parameters and bounded chunks. Check any
configured `open_basedir` before opening the file.

## Why

The descriptor follows the caller's transaction. A large file only needs one
chunk in memory. File paths never travel to PostgreSQL.

## Turned down

Server-side file access, an implicit transaction, or loading the whole file.
