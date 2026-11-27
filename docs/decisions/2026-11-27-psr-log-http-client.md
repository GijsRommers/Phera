# A native logger and HTTP client, and PSR adapters in plain PHP

- Date: 2026-11-27
- Area: Server and requests
- Decided by: Design

## Decision

Two native classes in `crates/runtime`, and one Composer package in PHP:

- **`Phera\Log\Logger`** (`crates/runtime/src/log/`): the eight PSR-3 level
  methods, `log()`, `withContext()`, `flush()`, `isHandling()`, and
  `Logger::memory()` with `records()` for tests. Lines are NDJSON
  (`{"ts":…,"level":…,"channel":…,"message":…,"context":{…}}`) or text
  (`[ts] channel.LEVEL: message {context}`); targets are `stderr`, `stdout`,
  `memory` or a file. `Phera\Log\Level` holds the level names.
- **`Phera\Http\Client`** (`crates/runtime/src/http_client/`): `request()`,
  the six verbs and `pool()` over Hyper's pooled client (HTTP/1.1 keep-alive,
  HTTP/2 by ALPN or prior knowledge), rustls with the system roots from
  `phera-io-pool`, redirects, timeouts, and `Phera\Http\ClientException`
  (a `RuntimeException`) when no response arrives. It answers with the
  router's own `Phera\Http\Response`, which gains `header($name, $default)`,
  `decodeJson()` and `Response::make()` (exact headers, no default
  `Content-Type`).
- **`packages/phera-psr`** (`phera/psr`): PSR-7 messages, streams and URIs,
  PSR-17 factories, `Psr15Bridge` to mount a PSR-15 handler and middleware on
  the router, `Psr18Client` over `Phera\Http\Client`, and `Psr3Logger` over
  `Phera\Log\Logger`. Its tests run offline on stub PSR interfaces
  (`tests/stubs`) and run in `cargo test` (`crates/cli/tests/http_client.rs`).

Both native classes are forked with a snapshot (`ClassHooks::fork`) and
refuse serialization. A client made at boot keeps its configuration in each
fork; its connections live in a per-thread pool keyed by the connection
settings, shared by every client on that thread.

## Why

- **The level check comes first.** A call below the minimum level returns
  from the method's ready path before the message or the context is read, so
  a disabled `debug()` costs a level comparison. A context that holds no
  `Stringable`, `JsonSerializable` or date object is formatted without leaving
  Rust; only one that does takes the async path, where PHP turns those
  objects into strings or arrays first.
- **File logging does not block requests on the disk.** A file has one writer
  thread for the whole process; loggers send it finished lines over a
  channel. It writes whatever is waiting in one go and flushes as soon as its
  queue is empty, so a quiet log is on disk at once and a busy one goes out in
  large writes. `flush()` and dropping the last logger of a file wait for the
  writer, so a CLI script's last lines are never lost.
- **One response class.** A handler returns a `Phera\Http\Response` and a
  client receives one; the PSR adapters convert to and from it. `json()`
  already names the static constructor, so the decoder is `decodeJson()`.
- **No new crates.** Hyper, hyper-util, rustls and tokio-rustls are already
  compiled for curl and the server; the runtime enables their client features.
- **PSR in PHP.** The PSR interfaces are PHP interfaces from Composer
  packages; a native class cannot implement an interface that may not be
  loaded. Thin PHP classes over the native ones keep `Psr\…` out of the
  runtime and work with the real `psr/*` packages.

## Turned down

- **Reusing curl's connector.** It carries proxies, DNS shares, cookie jars
  and curl's error codes. The client needs TCP, TLS with ALPN and a pool; a
  small connector of its own is easier to read.
- **A client per object holding its own connections.** A client forked into
  a request on another thread would carry connections whose tasks ran on the
  boot's runtime. The per-thread pool gives each worker its own and lets
  clients with the same settings share them.
- **Buffering file lines inside the logger.** A boot logger lives on in the
  worker's persistent VM, so a buffer there would hold a quiet server's lines
  until it filled.
- **Following redirects in `Psr18Client` by default.** PSR-18 returns a
  redirect response as it is; a `Phera\Http\Client` with `max_redirects`
  can be passed in.

## Structure: was, is

| Structure | Was | Is |
| --- | --- | --- |
| Logging | none native (Monolog in PHP) | `Logger` state: `Rc<Shared>` (channel, level, format, sink) + default context array |
| Log file writes | — | one writer thread per file, `mpsc` channel of finished lines, process-wide `HashMap<PathBuf, Sender>` |
| HTTP client pool | curl only | thread-local `Vec<(Transport, hyper Client)>`, scanned linearly (a handful of settings) |
| Client state | — | `Rc<Config>`, immutable, shared by forks |
| `Response` | made by handlers only | also made from a received response; `header()` default, `decodeJson()`, `make()` |
