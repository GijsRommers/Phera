# Cryptography (`openssl_*`, `sodium_*`, password hashing) is built from audited Rust crates

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Design

## Decision

Cryptography (`openssl_*`, `sodium_*`, password hashing) is built from audited Rust crates. Phera implements no cipher, hash, padding or secret comparison itself. What no audited crate offers is left out and listed.

## Why

Home-made cryptography is where runtimes get broken.

## Turned down

Linking OpenSSL or libsodium.
