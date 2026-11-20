# Compose NaCl boxes from RustCrypto and dalek

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Design

## Decision

Use crypto_secretbox for NaCl payloads. Derive box keys from X25519 with HSalsa20. For sealed
boxes, derive the nonce from both public keys and keep the recipient public key from the
supplied keypair.

## Why

PHP accepts constructed keypairs and non-canonical curve inputs. The selected secretbox crate
supplies the compatible wire format.

## Turned down

crypto_box 0.9 reduces the clamped scalar before multiplying the peer key. Reconstructing the
recipient public key changes the sealed-box nonce for constructed pairs.
