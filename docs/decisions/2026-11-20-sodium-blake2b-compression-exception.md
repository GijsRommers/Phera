# Implement only BLAKE2b’s compression function F in Phera

- Date: 2026-11-20
- Area: Safety and dependencies
- Decided by: Design (accepted in review on 2026-11-20; the owner can overrule it)

## Decision

Implement RFC 7693 §3.2 BLAKE2b F in safe Rust in `blake2b_compress.rs`. This is a narrow
exception to the cryptography policy. Production calls run exactly 12 rounds. Mixing uses
64-bit wrapping additions, XOR and fixed rotations. Message-word indices depend only on the
public round schedule.

## Why

RustCrypto blake2 0.10.6 and 0.11.0 expose neither compression nor a state round trip.
blake2b_simd 1.0.5 exposes Clone and count, with private chaining words and buffered contents.
Sodium needs a resumable, copyable state string.

## Turned down

REVM brings RustSec-flagged crypto-bigint 0.5.5. The alternate EVM crate introduces an
unacceptable supply-chain dependency. A whole-message buffer would be unbounded.

## Verification

[RFC 7693 Appendix A](https://www.rfc-editor.org/rfc/rfc7693.html#appendix-A), [EIP-152 vectors 4–7](https://eips.ethereum.org/EIPS/eip-152#test-cases), 4,144 streaming comparisons with blake2b_simd over empty through multi-megabyte messages and key, salt and personalisation combinations, and the existing PHP oracle. Vector 8 passed in a separate 4,294,967,295-round run. Automated tests exercise the bounded round cases.
