# Compose Sodium secretstream from HChaCha20, ChaCha20 and Poly1305

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Design

## Decision

Use RustCrypto primitives with libsodium’s transcript, ciphertext-dependent zero padding and
ratchet. Serialize the key, counter, nonce and checksum as a 52-byte opaque string. Verify the
MAC before decryption or advancing the state.

## Why

This preserves wire interoperation and the ability to copy, validate and explicitly rekey a
PHP state.

## Turned down

Standard IETF AEAD for unaligned secretstream messages, or the RustCrypto secretstream wrapper
with private rekey and state.
