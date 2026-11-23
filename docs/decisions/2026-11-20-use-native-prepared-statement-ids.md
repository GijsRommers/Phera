# Use native prepared statement IDs

- Date: 2026-11-20
- Area: Speed and method
- Decided by: Design

## Decision

Keep the native statement handle returned by each SQL client. Native
execution uses that handle and its server-side statement ID or name.
PostgreSQL still prepares on the first execution where PHP does.
Emulated prepares bind into SQL text for each execution.

## Why

The native path already sends a handle rather than resolving SQL text on
every execution. PDO emulation has separate observable behavior, including
when errors occur and how literal parameters are quoted.

## Turned down

Adding another table of statement IDs or a lookup cache keyed by SQL.
