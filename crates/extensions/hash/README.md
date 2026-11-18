# Hashing and passwords in Phera

This crate is PHP's hash extension: `hash()` and its family with all 60 of
PHP 8.5's algorithms, HMAC, PBKDF2 and HKDF, incremental hashing through
`HashContext`, `md5()` and `sha1()`, `hash_equals()`, the password functions
and `crypt()`. Results, errors and warnings match PHP's byte for byte, except
where listed below.

```php
hash('sha256', 'hello');
hash_hmac('sha256', $message, $key);
$h = hash_init('xxh3', options: ['seed' => 42]);
hash_update($h, $chunk);
$digest = hash_final($h);
password_verify($password, password_hash($password, PASSWORD_ARGON2ID));
```

`crc32()` itself belongs to the standard extension.

## What runs where

Work that is slow on purpose runs on Tokio's blocking threads, so the
worker thread serves other requests meanwhile: `password_hash()`,
`password_verify()`, `crypt()` and `hash_pbkdf2()`. The `*_file()` functions
and `hash_update_file()` read the file asynchronously, 8 KiB at a time.
Everything else is computed on the request's thread, as PHP does.

The cryptographic hashes come from the RustCrypto crates (`md-5`, `sha2`,
`sha3`, ...) and xxHash from `xxhash-rust`; bcrypt, Argon2 and the `crypt()`
schemes from `bcrypt`, `argon2` and `pwhash`. Tiger, Snefru, HAVAL,
MurmurHash3 and the checksums are ported from php-src and keep its licence,
in `LICENSE-PHP`.

## Differences from PHP

- **Files** are local paths only: stream wrappers (`php://`, `http://`,
  `compress.zlib://`) are not supported yet, and `hash_update_stream()`
  always fails, because Phera has no stream resources.
- **Tiger, Snefru, HAVAL, MurmurHash3, xxHash and the checksums** are
  computed in one go. A `HashContext` for one of them keeps all its input
  until `hash_final()`, and `hash_file()` holds the whole file in memory.
- **`HashContext` objects** cannot be cloned (a clone is finalized) or
  serialized.
- **`crypt()`** does not know bcrypt's `$2x$` variant and returns `*0` for
  it; `$2a$`, `$2b$`, `$2y$`, MD5, SHA-256, SHA-512, DES and extended DES
  work.
- **A short xxh3 or xxh128 secret** (under 136 bytes) is an error for any
  input. PHP 8.5.12 raises the same error for up to 241 bytes of input and
  hangs on longer input; the test `tests/oracle/xxhash_secrets.rs` records
  that.

## How it is checked

`cargo test -p phera-ext-hash` compares the extension with PHP 8.5, found
as described in CONTRIBUTING.md: every algorithm on every input length from
0 to 300 bytes and around the block sizes up to 1 MiB, in binary and in
hexadecimal (36,960 calls); HMAC with keys on both sides of every block
size (264,880); incremental hashing split at every point, with copies
(24,120); KDFs, seeds, secrets and argument errors (1,626); files (39);
`crypt()` (56); password options and malformed hashes (44). Password hashes
are checked across both runtimes: PHP verifies Phera's and Phera verifies
PHP's. The php-src test vectors (494) run without PHP.
