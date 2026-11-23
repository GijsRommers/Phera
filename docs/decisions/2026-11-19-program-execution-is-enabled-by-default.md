# Program execution is enabled by default

- Date: 2026-11-19
- Area: Server and requests
- Decided by: Design

## Decision

Program execution is enabled by default. `phera serve --no-process` makes every program launch entry point throw `Error`, including `shell_exec()` and backticks.

## Why

Frameworks expect process functions to be available. Deployments that never launch children can disable them together.

## Turned down

Disabling them by default or requiring a list of individual functions.
