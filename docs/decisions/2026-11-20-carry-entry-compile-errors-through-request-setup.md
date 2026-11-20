# Carry entry compile errors through request setup

- Date: 2026-11-20
- Area: Server and requests
- Decided by: Codex

## Decision

Run entry compilation through the worker-local cache and retain any compile error until request setup has configured diagnostics and multipart cleanup.

## Why

PHP's error-display rules apply to entry compilation. The VM needs request headers for logging and must own any temporary uploads until request teardown.

## Turned down

Returning from the worker cache with a raw compiler error before the request is configured.
