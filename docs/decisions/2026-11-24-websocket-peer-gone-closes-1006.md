# A WebSocket peer that vanishes closes with 1006

- Date: 2026-11-24
- Area: Server and requests
- Decided by: Design

## Decision

When the other side of a WebSocket goes away without a close frame (a dropped
TCP connection, a reset HTTP/2 stream), `onClose` receives code `1006` and an
empty reason, and the server sends nothing. Before, an HTTP/1.1 socket that
reached end of file reported `1002` ("protocol error"), because
`tokio-tungstenite` reports that case as
`ProtocolError::ResetWithoutClosingHandshake`
(`crates/server/src/websocket/stream.rs`).

## Why

RFC 6455 section 7.1.5 names `1006` for a connection closed without a close
frame, and browsers report it so. With WebSockets over HTTP/2 the same drop
already gave `1006`; an application must not see different codes for the two
transports.

## Turned down

- **Keeping `1002` on HTTP/1.1.** It says the peer broke the protocol, which
  it did not, and there is nobody left to send the close frame to.

## Structure: was, is

| Structure | Was | Is |
| --- | --- | --- |
| Read error mapping in `drive_io` | every `Error::Protocol` gives `1002` and a close frame | `ResetWithoutClosingHandshake` gives `1006` without a frame; other protocol errors stay `1002` |
