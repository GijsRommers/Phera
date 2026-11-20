# Use RustCrypto base16ct for Sodium hexadecimal codecs

- Date: 2026-11-20
- Area: Safety and dependencies
- Decided by: Design

## Decision

Use the already-locked RustCrypto base16ct for lowercase encoding, mixed-case decoding and
Sodium test fixtures.

## Why

The same codec covers both conversion directions. The PHP boundary handles ignored separators
and error messages, checked by the oracle.

## Turned down

Keep hex, by KokaKiwi and contributors, as a separate codec dependency.
