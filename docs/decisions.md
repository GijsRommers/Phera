# Design decisions

## 2026-11-19 — Register Sodium as a native extension

Register the PHP module and exception through the extension API. Use the existing
`base64` crate and the `hex` crate for the encoding functions. Match Sodium's
variant identifiers and ignore-string rules at the PHP boundary. The standard
extension's base64 decoder has different padding and invalid-input semantics.
The PHP oracle loads Sodium in both hosted test jobs so those semantics are
covered on Linux.

## 2026-11-19 — Use RustCrypto for Sodium's cryptographic operations

Use `crypto_secretbox` and `crypto_box` for the NaCl wire formats. Sealed boxes
derive their nonce with BLAKE2b over both public keys and use the recipient
public key from the PHP keypair during opening. Deriving a replacement public
key during opening would change the behavior of constructed keypairs. Reuse
OpenSSL's AES-GCM, ChaCha20-Poly1305, dalek, SHA-2 and randomness dependencies.
Use the stable NaCl crates. Their newer prereleases pin incompatible prerelease
versions of the shared cipher and curve crates.

Use the IETF AEAD interfaces for ChaCha20 and XChaCha20. The original
eight-byte-nonce AEAD uses the legacy ChaCha20 and Poly1305 implementations
with Sodium's length-field order. An IETF nonce with four prepended zeroes
produces a different authentication tag. The XChaCha20 stream's explicit
counter uses the separate 64-bit cipher interface, including Sodium's wrap
at the maximum counter. AES-256-GCM availability includes RustCrypto's portable
backend.

Use BLAKE2b parameter blocks for Sodium's KDF and BLAKE2b over the X25519 shared
secret and both public keys for KX. HKDF and labeled HMAC give different keys.
Authentication truncates HMAC-SHA-512 to 32 bytes. SipHash-2-4 returns the
eight little-endian bytes produced by Sodium.

## 2026-11-19 — Reuse Argon2 and implement the scrypt password format

Reuse the same Argon2 implementation as PHP's existing `password_hash()`
support. Sodium has its own memory units, default algorithm, and parameter
validation. Run Argon2 and scrypt off the request thread. Translate Sodium's
scrypt work limits into its `N`, `r` and `p` parameters and encode the
`$7$` string format. The scrypt crate's PHC string format does not
interoperate with PHP's scrypt password strings.

## 2026-11-19 — Preserve reference mutation and copy-on-write

Use constant-time comparisons from `subtle` and `crypto-bigint`. Increment
and add use the latter's fixed-precision unsigned arithmetic. Erase a PHP
string in place when its backing buffer has a single owner, and set the
reference to null. Shared values retain their bytes. Wiping a temporary copy
would leave the original allocation intact.

## 2026-11-19 — Publish the implemented PHP surface

Use the Sodium 1.0.22 constant values observed with the local PHP 8.5 oracle.
Its library version constants describe the compatibility target. Sodium is a
Rust module, so it has no linked libsodium version to query. The PHP patch
does not govern the library version reported by a PHP build.

Streaming BLAKE2b requires a compatible string representation for the internal
state. The selected BLAKE2b crate encapsulates its state. Buffering the entire
message would give streaming callers unbounded memory use, and an external
handle would change PHP copy and serialization behavior. Defer those calls
until the state can be represented safely and compatibly.

Defer AEGIS and secretstream until a compatible audited pure Rust backend is
available. The RustCrypto secretstream API keeps explicit rekey and state
serialization private. Wrapping it as a PHP string would also change cloning
and serialization. The available `block-padding` ISO 7816-4 decoder exits early
on padding bytes, so it cannot provide Sodium's constant-time unpadding.
