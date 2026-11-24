# Serve HTTP/1.1 and HTTP/2 on one listener

- Date: 2026-11-23
- Area: Server and requests
- Decided by: Design

## Decision

Enable `hyper`'s `http2` feature and serve both HTTP/1.1 and HTTP/2 on the same
listener through `hyper_util::server::conn::auto::Builder` with a local
`LocalSet` executor (`LocalExecutor`), `HttpVersion::Http2`, and
`$_SERVER['SERVER_PROTOCOL'] = 'HTTP/2.0'`. On plain TCP, Hyper selects HTTP/2
when the connection starts with the HTTP/2 prior-knowledge preface (`h2c`), and
otherwise serves HTTP/1.1. Synthesize a lower-case `host` header from the
HTTP/2 `:authority` pseudo-header when `Host` is absent, strip hop-by-hop
connection headers from HTTP/2 responses, and enforce the initial read timeout
across protocol detection via `PrefaceTimeoutStream`.

`phera serve` does not terminate TLS today (`tokio-rustls` in `crates/cli` is
used only by `phera upgrade` HTTP downloads). When TLS termination is added to
the server, `rustls::ServerConfig::alpn_protocols` will advertise `h2` and
`http/1.1` into the same `auto::Builder` path.

## Why

Reverse proxies speak HTTP/2 (`h2c` prior knowledge over plain HTTP inside a
cluster or pod) to their upstreams to multiplex requests onto fewer connections.
`hyper_util::server::conn::auto` shares one acceptor, worker pool, and graceful
shutdown watcher across both protocols without penalizing HTTP/1.1 requests.

## Turned down

- **RFC 7540 HTTP/1.1 `Upgrade: h2c` cleartext upgrade**: deprecated in RFC 9113
  and unused by modern proxies; proxies use prior-knowledge `h2c` or TLS ALPN.
- **Spawning a second listener port for HTTP/2**: splits ports, health probes,
  and per-worker connection limits.

## Structure: was, is

| Structure | Was | Is |
| --- | --- | --- |
| Connection handler (`crates/server/src/connection.rs`) | `hyper::server::conn::http1::Builder` | `hyper_util::server::conn::auto::Builder` with `LocalExecutor` |
| Initial read guard (`crates/server/src/receive_timeout.rs`) | `http1::header_read_timeout` only | `PrefaceTimeoutStream` before `http1` / `http2` |
| `HttpVersion` (`crates/server/src/request.rs`) | `Http10`, `Http11` | `Http10`, `Http11`, `Http2` (`"HTTP/2.0"`) |
