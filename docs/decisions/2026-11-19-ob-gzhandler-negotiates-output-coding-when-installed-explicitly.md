# `ob_gzhandler` negotiates output coding when installed explicitly

- Date: 2026-11-19
- Area: Native math and compression
- Decided by: Design

## Decision

`ob_gzhandler` negotiates output coding when installed explicitly. The `zlib.output_compression` INI setting stores configuration.

## Why

The runtime already supports explicit PHP output buffering and independent HTTP compression.

## Turned down

Installing a new output buffer at request startup.
