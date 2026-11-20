# Use Sodium’s hash and key-derivation constructions

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Design

## Decision

Use BLAKE2b parameter blocks for KDF and BLAKE2b over the X25519 shared secret and both public
keys for KX. Truncate HMAC-SHA-512 to 32 bytes for authentication. Use SipHash-2-4 with a
little-endian result for shorthash.

## Why

These constructions produce the same bytes as PHP and suit applications exchanging keys and
authentication tags.

## Turned down

HKDF, labelled HMAC or a native-endian shorthash result.
