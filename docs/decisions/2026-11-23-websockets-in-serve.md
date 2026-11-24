# WebSockets in phera serve

- Date: 2026-11-23
- Area: Server and runtime
- Decided by: Design

## Decision

`phera serve` owns idle WebSocket sockets and RFC 6455 framing in Rust
(`crates/server/src/websocket/`) using `tokio-tungstenite`, and executes each
`onOpen`, `onMessage`, and `onClose` lifecycle event in an isolated short-lived
PHP run (`Snapshot::fork()` or classic fallback) rather than pinning long-lived
PHP handler objects across idle connections.

- On the HTTP upgrade request, PHP handles authentication/cookies normally and
  calls `Phera\WebSocket\accept(Handler::class, context: ..., rooms: ..., subprotocol: ...)`.
  Any normal HTTP response returned without calling `accept()` (such as 401 or
  404) is sent as-is without upgrading.
- Events for a single connection run strictly sequentially in arrival order via
  a bounded per-connection queue, while different connections run concurrently.
- Snapshotted applications resolve handlers via `Phera\WebSocket\resolveUsing(callable)`
  (registered automatically by the Laravel and Symfony adapters before
  `Phera\snapshot()`) or `new $handlerClass()`. Event forks never resume
  bytecode after `Phera\snapshot()`. Un-snapshotted scripts run with
  `Phera\WebSocket\isEvent() === true` so top-level declarations load without
  re-running `accept()`.
- Connection context bags (`accept(..., context: ...)` / `$conn->context()` /
  `$conn->setContext()`) are serialized in Rust (`crates/runtime/src/websocket/context.rs`)
  with only `null`, scalars, and nested arrays allowed (`TypeError` otherwise),
  bounded by `phera.ws_max_context_bytes` (default 16 KiB).
- Room membership, direct sends, and cross-worker broadcasts go through a
  64-sharded `WebSocketHub` reachable from both WebSocket event handlers and
  ordinary HTTP requests. Sending invalid UTF-8 as a text message (`$binary = false`)
  from PHP throws `ValueError` at `send()`/`broadcast()` time rather than
  dropping the message silently.
- Connection IDs are sequential `ws-<n>` identifiers addressed only by
  application PHP code and must never be treated as secret bearer tokens.
(`crates/server/src/websocket/`, `crates/vm/src/websocket.rs`, `crates/runtime/src/websocket/`, `crates/cli/src/serve.rs`)

## Why

Holding one PHP VM or framework container per idle socket would cost megabytes
of per-connection memory and reintroduce long-lived mutable state bugs across
messages. Keeping sockets and rooms in Rust yields a few kilobytes of idle
memory per socket while giving every WebSocket message the same clean isolation,
framework container access, and zero cross-connection leakage as an HTTP fork.

## Turned down

- Running a persistent PHP event loop or pinning a long-lived `Vm` per WebSocket
  connection (high idle memory and leaked request state across messages).
- Resuming snapshotted HTTP front controllers after `Phera\snapshot()` for
  WebSocket frames (would force HTTP router/kernel dispatch on every message).
