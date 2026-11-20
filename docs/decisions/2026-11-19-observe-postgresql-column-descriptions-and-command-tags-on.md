# Observe PostgreSQL column descriptions and command tags on the existing socket. Have PostgreSQL format prepared binary rows and launch async sends on the lease’s Tokio I/O runtime

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Observe PostgreSQL column descriptions and command tags on the existing socket. Have PostgreSQL format prepared binary rows and launch async sends on the lease’s Tokio I/O runtime.

## Why

Keeps command status and session-dependent formats without executing statements twice. A busy PHP polling loop leaves I/O on its own runtime. Prepared rows currently cost an extra round trip per row.

## Turned down

Another PostgreSQL client or reconstructing session-dependent formats in Rust.
