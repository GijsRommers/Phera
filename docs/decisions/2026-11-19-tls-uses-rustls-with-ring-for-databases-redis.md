# TLS uses rustls with ring for databases, Redis and HTTP clients

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Owner

## Decision

TLS uses rustls with ring for databases, Redis and HTTP clients. PDO, Redis, and
curl share TLS 1.2 and 1.3 policy in `phera-io-pool`.

## Why

Ring is audited, widely deployed and already in the dependency tree. Its bundled C and assembly sit behind safe Rust APIs. The removed pure-Rust provider was a pre-release that labeled itself unsuitable for production.

## Turned down

Keeping the pre-release provider or linking OpenSSL for TLS.
