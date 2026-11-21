# Retain pending stream TLS handshakes

- Date: 2026-11-21
- Area: Streams
- Decided by: Design

## Decision

A TCP stream holds the active rustls client or server handshake. On a
nonblocking stream, `stream_socket_enable_crypto()` polls it once and
returns zero while it needs I/O. The next call keeps negotiating from the
same state. Ordinary stream reads and writes use the established TLS channel.

The shared runtime TLS policy supplies TLS 1.2 and 1.3. Client verification
checks both trust and the host name by default. Custom CA material,
certificate pins, depth limits, and local credentials apply to the
handshake that opens the connection.

## Why

TLS negotiation consumes bytes from both peers before the handshake completes.
Keeping the handshake lets PHP resume it without sending a second ClientHello
or losing bytes already read from the socket.

## Turned down

Restarting negotiation after each nonblocking call.
