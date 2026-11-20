# Curl uses the Rust HTTP stack

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Design

## Decision

Curl uses Hyper for HTTP/1.1 and HTTP/2, Tokio for asynchronous I/O, and rustls
for TLS. It supports `http` and `https` and refuses other schemes with libcurl's
unsupported-protocol error.

## Why

The runtime's scheduler handles socket, timer, and PHP callback waits. Refusing
`file://`, `gopher://`, and similar protocols removes a class of request-forgery
paths.

## Turned down

Binding libcurl with a separate event loop and PHP callback bridge, and
enabling non-HTTP protocols.
