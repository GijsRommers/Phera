# The decoder primes its DEFLATE history with a non-final stored block for preset dictionaries

- Date: 2026-11-19
- Area: Native math and compression
- Decided by: Design

## Decision

The decoder primes its DEFLATE history with a non-final stored block for preset dictionaries.

## Why

References to dictionary bytes decode with the shared backend while PHP's reported input length and checksums start at the caller's data.

## Turned down

Replacing the server's backend solely for dictionary and compression-strategy support.
