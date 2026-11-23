# Use Hyper for HTTP stream responses

- Date: 2026-11-20
- Area: Streams
- Decided by: Design

## Decision

Opening an HTTP URL with `fopen()` uses Hyper's HTTP/1 client over the
same TCP and rustls connections as stream sockets. The initial wrapper
returns a read-only buffered response with a 16 MiB body limit. It
provides the response headers through `stream_get_meta_data()`.

## Why

Guzzle's stream handler opens the URL and reads the response metadata
through PHP's stream API. Existing workspace HTTP framing and TLS
components handle the exchange on the worker's reactor.

## Turned down

Implementing HTTP framing or parsing directly over raw socket bytes.
