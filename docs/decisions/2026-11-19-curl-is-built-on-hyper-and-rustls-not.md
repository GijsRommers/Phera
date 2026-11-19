# curl is built on `hyper` and rustls, not on libcurl

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Design

## Decision

curl is built on `hyper` and rustls, not on libcurl. Only `http` and `https` exist; other protocols are refused with libcurl's error.

## Why

Same reason as DOM. Refusing `file://`, `gopher://` and the like also closes a class of request-forgery problems.

## Turned down

Binding libcurl.
