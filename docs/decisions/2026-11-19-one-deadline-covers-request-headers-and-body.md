# One deadline covers request headers and body

- Date: 2026-11-19
- Area: Server and requests
- Decided by: Design

## Decision

One deadline covers request headers and body. `--request-read-timeout` sets the initial budget, 60 seconds by default. Each 16 KiB received in the body adds a second. Expiry returns HTTP 408.

## Why

Credit for bytes received gives steady uploads time to finish. A trickle earns little credit, and the body-size limit bounds the extension.

## Turned down

A timeout that resets on each read, or a fixed deadline with no credit for upload progress.
