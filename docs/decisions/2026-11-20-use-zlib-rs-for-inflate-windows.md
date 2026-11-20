# Use zlib-rs for inflate windows

- Date: 2026-11-20
- Area: Extensions
- Decided by: Design

## Decision

The inflate filter uses the safe `zlib-rs` streaming API to accept raw,
RFC 1950 zlib, gzip, and auto-detected headers under PHP's window option.
The deflate filter retains the existing `flate2` backend and uses PHP's
flush modes and 32 KiB output buffer. Exact compressed bytes remain open.

## Why

The earlier boolean header setting lost the distinction between gzip and
zlib. The Trifecta Tech Foundation maintains `zlib-rs` and its safe API
exposes both the header formats and window selection. Its encoder also
produced different blocks in the PHP byte comparisons.

## Turned down

Changing the compression backend across the workspace to fix stream filters.
