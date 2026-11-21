# Keep initial stream sockets request owned

Superseded by [Reuse clean persistent stream sockets](2026-11-21-reuse-clean-persistent-stream-sockets.md).

- Date: 2026-11-20
- Area: Streams
- Decided by: Design

## Decision

The initial `pfsockopen()` and `STREAM_CLIENT_PERSISTENT` entry points
open request-owned streams. They expose the regular socket operations.
Worker-wide connection persistence needs a separate pool keyed by the
socket target, TLS policy and application ownership.

## Why

A retained descriptor can carry unread bytes and application protocol
state. Socket reuse needs explicit ownership at the worker boundary.

## Turned down

A process-wide table of descriptors indexed only by host and port.
