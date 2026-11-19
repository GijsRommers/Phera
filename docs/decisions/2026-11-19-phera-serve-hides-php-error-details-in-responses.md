# `phera serve` hides PHP error details in responses and logs them by default

- Date: 2026-11-19
- Area: Server and requests
- Decided by: Owner

## Decision

`phera serve` hides PHP error details in responses and logs them by default. `--dev` enables display.

## Why

Production logs retain diagnostics, including fatal traces. Local development can show the same errors in the response.

## Turned down

Displaying PHP errors in production by default.
