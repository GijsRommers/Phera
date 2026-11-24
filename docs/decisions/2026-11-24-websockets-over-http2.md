# WebSockets over HTTP/2 (RFC 8441 extended CONNECT)

- Date: 2026-11-24
- Area: Server and requests
- Decided by: Design

## Decision

`phera serve` accepts WebSockets on cleartext HTTP/2 connections as well as
through the HTTP/1.1 upgrade. The server announces
`SETTINGS_ENABLE_CONNECT_PROTOCOL = 1`; a stream that opens with
`:method CONNECT` and `:protocol websocket` becomes one WebSocket, next to
ordinary requests and other WebSockets on the same connection. It is always
on, because HTTP/2 itself has no switch.

- **One PHP side.** The request reaches PHP as the HTTP/1.1 upgrade would:
  `REQUEST_METHOD` is `GET`, `HTTP_UPGRADE` is `websocket`, `HTTP_CONNECTION`
  is `Upgrade`, `HTTP_HOST` comes from `:authority`. `SERVER_PROTOCOL` is
  `HTTP/2.0`, the request attribute that already told the transports apart.
  The same `accept()`, origin check, authentication, rooms, event forks and
  `WebSocketConfig` limits apply. `accept()` answers `200` (not `101`) with the
  headers PHP set and the negotiated `Sec-WebSocket-Protocol`; there is no
  `Sec-WebSocket-Key`/`Accept` on HTTP/2 (RFC 8441 section 5).
- **One framing path.** The upgraded stream goes into the same
  `run_upgraded_websocket` loop (`tokio-tungstenite`'s
  `WebSocketStream::from_raw_socket`) as an upgraded HTTP/1.1 socket.
- **How hyper exposes it** (hyper 1.12.0, hyper-util 0.1.21, h2 0.4.20,
  tokio-tungstenite 0.30.0): `auto::Builder::http2().enable_connect_protocol()`
  sends the setting; the `:protocol` pseudo-header arrives as the request
  extension `hyper::ext::Protocol`; a CONNECT request has an empty body and an
  `OnUpgrade` extension, and `hyper::upgrade::on(&mut request)` resolves to a
  `hyper::upgrade::Upgraded` once the service answers 2xx without a body.
  Hyper bridges that `Upgraded` to the stream's h2 `SendStream`/`RecvStream`
  (`proto/h2/upgrade.rs`): a write waits for stream window, a read releases
  window as it is consumed.
- **Flow control and memory.** Each stream keeps hyper's 1 MiB receive window;
  the connection window is eight of them (8 MiB). A WebSocket whose PHP handler
  falls behind stops being read, so it holds at most one stream window of the
  connection window; seven such streams can stall and the rest still move.
  With hyper's defaults (1 MiB and 1 MiB) the stalled streams share one
  window's worth of connection window: in the test, three of them freeze the
  DATA of every other stream on the connection (with one, the neighbour
  still got through in that test; the margin is not something to rely on). On the
  sending side a client that stops reading exhausts only its own stream window:
  the write deadline or the bounded send queue drops that WebSocket (1006 or
  1008), exactly as for a full HTTP/1.1 socket.
- **Closing.** After the close handshake (or a failure) the loop shuts the
  transport down before `onClose` runs: END_STREAM on HTTP/2, FIN on HTTP/1.1.
  The connection and its other streams carry on. When the TCP connection
  drops, every WebSocket stream on it reads an error and runs `onClose` with
  `1006`. Shutdown sends `1001` on each stream, then hyper's GOAWAY.
- **Refusals.** An extended CONNECT for another protocol is answered `501`
  before PHP runs; `Sec-WebSocket-Version` other than 13 is answered `400`
  with `Sec-WebSocket-Version: 13` (HTTP/2 has no `426` with `Upgrade`). A
  CONNECT without `:protocol` is handled as before. When PHP answers without
  calling `accept()` (a page served at the same path), a non-2xx status goes to
  the client unchanged, but a 2xx becomes `400`: on HTTP/2 a 2xx answer to an
  extended CONNECT is the acceptance itself, so the client would read the
  page as WebSocket frames.
- **TLS.** `phera serve` has no TLS, and browsers use RFC 8441 only over TLS
  with ALPN `h2`. Over cleartext this serves a proxy that speaks `h2c` to its
  upstream; when TLS arrives (architecture step 10) the same path applies.

## Why

A proxy that already multiplexes requests over one HTTP/2 connection to Phera
can carry WebSockets on it too, instead of opening a TCP connection per
socket. Keeping the request PHP sees identical means routes written for
`GET /chat` with `Upgrade: websocket`, and the authentication in them, work
unchanged.

## Turned down

- **Passing `CONNECT` through to PHP.** Routers match the upgrade as `GET`;
  RFC 8441 defines the CONNECT as the HTTP/2 form of that same request.
- **A second WebSocket implementation on h2 frames.** The byte stream hyper
  gives is enough for the existing loop; two framings would drift apart.
- **A switch for it.** HTTP/2 has none; an HTTP/2 client that does not want
  WebSockets just does not send `:protocol`.
- **A smaller stream window** to protect the connection window: it would slow
  every ordinary upload over a link with latency.
- **Resetting a stream that was dropped for its write deadline.** hyper's
  `Upgraded` cannot send RST_STREAM; hyper's send task keeps one buffered
  write until the client reads, resets the stream, or the connection ends.
  That is bounded (one write per stream, at most 200 streams per connection).

## Structure: was, is

| Structure | Was | Is |
| --- | --- | --- |
| HTTP/2 settings (`crates/server/src/connection.rs`) | hyper defaults: no extended CONNECT, 1 MiB stream and 1 MiB connection window | `enable_connect_protocol()`, 1 MiB stream window, 8 MiB connection window |
| WebSocket request kinds (`connection.rs`) | HTTP/1.1 upgrade only (`!http2 && is_websocket_upgrade_request`) | `WebSocketTransport::{Http1Upgrade, Http2Connect}`; HTTP/2 sorted by `extended_connect::classify` (one method comparison for an ordinary request) |
| Accepted response (`websocket/handshake.rs`, `websocket/extended_connect.rs`) | `build_101_response` | shared `keep_application_headers` and `push_negotiated_subprotocol`; `build_101_response` and `build_200_response` |
| Connection slot of a WebSocket (`websocket/stream.rs`) | `tcp_slot: ConnectionSlot`, always moved from the connection | `Option<ConnectionSlot>`: moved on HTTP/1.1, `None` on HTTP/2 (the connection keeps it) |
| End of the transport (`websocket/stream.rs`) | dropped after `onClose` finished | shut down right after the close handshake, before `onClose` |
