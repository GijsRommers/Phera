# Reuse the PDO unbuffered cursor and allow native iterator steps to await input. Use it for mysqli single statements

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Reuse the PDO unbuffered cursor and allow native iterator steps to await input. Use it for mysqli single statements.

## Why

Each fetch or iterator advance can read one row while the worker serves other requests. The cursor owns the pending reply until it is consumed or drained, which preserves affected-row reporting and connection reuse.

## Turned down

Buffering at cursor creation or blocking the worker during iteration.
