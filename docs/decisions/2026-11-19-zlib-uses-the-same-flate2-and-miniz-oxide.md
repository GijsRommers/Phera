# zlib uses the same `flate2` and `miniz_oxide` backend as the server

- Date: 2026-11-19
- Area: Native math and compression
- Decided by: Design

## Decision

zlib uses the same `flate2` and `miniz_oxide` backend as the server. It frames gzip and zlib members explicitly and emits stored blocks directly for one-shot level zero.

## Why

Framing controls checksums, consumed byte counts, and the decompression limit. The version constants describe the targeted zlib 1.2.12 interface.

## Turned down

Binding the platform's C zlib and tying behavior to its version.
