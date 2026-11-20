# Prepare immutable callable facts on compiled units

- Date: 2026-11-19
- Area: Server and requests
- Decided by: Codex

## Decision

Prepare callable facts, lowercase name text, and type scope classifications once per cached unit. Bind their indexes in each request.

## Why

These facts depend only on bytecode. PHP names, linked classes, and mutable lookup cells depend on what the request has executed.

## Turned down

Sharing resolved classes or call sites across requests.
