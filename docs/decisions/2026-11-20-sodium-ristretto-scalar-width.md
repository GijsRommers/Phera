# Retain crypto-bigint for Ristretto scalar wrapping

- Date: 2026-11-20
- Area: Safety and dependencies
- Decided by: Design

## Decision

Use RustCrypto crypto-bigint for the unsigned 256-bit wrap before modular reduction in
Ristretto scalar add and subtract.

## Why

Reducing each input first changes the result on non-canonical inputs that overflow 256 bits.

## Turned down

Reducing before adding or maintaining a second carry loop specifically for Ristretto.
