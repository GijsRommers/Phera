# Use the cipher variants required by Sodium

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Use the RustCrypto IETF and XChaCha20 AEADs. Compose the original eight-byte ChaCha20 AEAD
with legacy ChaCha20 and Poly1305. Use the 64-bit stream interface for explicit XChaCha20
counters. Gate AES-GCM availability on AES and polynomial-multiply CPU features.

## Why

The original AEAD has its own length-field order. Explicit counters wrap, and PHP reports
AES-GCM availability according to CPU features.

## Turned down

Padding the original nonce to its IETF width, truncating stream counters, or advertising
software AES-GCM on unsupported CPUs.
