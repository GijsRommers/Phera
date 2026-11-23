# Reuse clean persistent stream sockets

- Date: 2026-11-21
- Area: Streams
- Decided by: Design

## Decision

Each worker keeps a bounded list of idle persistent connections. The
application entry path, persistent ID, endpoint, source address, and
TLS options identify a connection. Changes to certificate or key files
invalidate the match. A request holds its connection exclusively until
the resource is released. PHP aliases in that request share one handle.

The worker drops a closed socket or one with unread data, rather than
letting buffered replies cross the request boundary. An explicit
`fclose()` closes the connection. Upgrades of plain persistent streams
are retired after use because the negotiated state changes their policy.

A boot can keep a persistent socket's dial settings. The fork connects
on its first I/O. A boot that still holds a live descriptor is refused.

## Why

Arbitrary socket protocols have no common command to reset their input
or session state. Checking the local buffers and descriptor before reuse
keeps handles and queued input isolated while reusing clean connections.

## Turned down

Sharing a descriptor between active requests, or keeping a process-wide
table indexed by host and port alone.
