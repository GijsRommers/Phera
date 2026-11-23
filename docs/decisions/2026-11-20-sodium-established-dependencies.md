# Use established Rust dependencies for Sodium

- Date: 2026-11-20
- Area: Safety and dependencies
- Decided by: Design

## Decision

Use the workspace’s established RustCrypto, dalek, and other maintained Rust libraries for the
operations listed below.

## Why

Sodium uses the published APIs of these libraries. Each direct dependency has a specific role
in PHP compatibility or its verification.

## Turned down

An EVM dependency for streaming hash compression.

## Direct dependency inventory

| Dependency | Maintainer | Use in Sodium |
| --- | --- | --- |
| `aes-gcm` | RustCrypto | AES-256-GCM authenticated encryption. |
| `argon2` | RustCrypto | Argon2i and Argon2id password hashing and verification. |
| `base16ct` | RustCrypto | Hexadecimal encoding and decoding. |
| `base64` | Marshall Pierce and contributors | Established codec shared by the workspace. Provides all four PHP Base64 variants. |
| `blake2b_simd` | Jack O’Connor, BLAKE3 co-author | Parameterised BLAKE2b one-shot hashing, KDF, KX, sealed-box nonces, state checksums, and the streaming test oracle. |
| `block-padding` | RustCrypto | ISO/IEC 7816-4 padding. |
| `chacha20` | RustCrypto | ChaCha20, XChaCha20 and HChaCha20 for streams, legacy AEAD and secretstream. |
| `chacha20poly1305` | RustCrypto | ChaCha20-Poly1305 IETF and XChaCha20-Poly1305 IETF AEAD. |
| `crypto-bigint` | RustCrypto | Fixed-width wrap before reduction for Ristretto scalar add and subtract. |
| `crypto_secretbox` | RustCrypto | XSalsa20-Poly1305 secretboxes and NaCl box payloads. |
| `curve25519-dalek` 5 | dalek-cryptography | Ristretto points and scalars, and Ed25519 point decoding and conversion. |
| `ed25519-dalek` | dalek-cryptography | Ed25519 signing, verification and key derivation. |
| `getrandom` | Rust Random Project | OS randomness for keys, salts, headers and ephemeral secrets. |
| `hmac` | RustCrypto | HMAC-SHA-512 for Sodium authentication. |
| `phera-extension-api` | Phera | PHP functions, values, reference arguments and exception registration. |
| `poly1305` | RustCrypto | MAC computation for the original ChaCha20 AEAD and secretstream. |
| `salsa20` | RustCrypto | XSalsa20 streams and HSalsa20 box key derivation. |
| `scrypt` | RustCrypto | The memory-hard KDF for Sodium's scrypt password APIs. |
| `sha2` | RustCrypto | SHA-512 for authentication, Ed25519 key conversion and seeded boxes. |
| `siphasher` | Frank Denis, libsodium maintainer | SipHash-2-4 for `sodium_crypto_shorthash`. |
| `subtle` | dalek-cryptography | Constant-time comparisons and selection, including the unpadding scan. |
| `tokio` | Tokio project | Runs password hashing on the blocking pool. |
| `x25519-dalek` | dalek-cryptography | X25519 public keys and Diffie–Hellman for boxes and key exchange. |
| `zeroize` | RustCrypto | Clears owned key bytes and temporary state. |
