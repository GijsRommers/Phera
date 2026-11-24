# Refine each alternative of a positive type test

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Keep only the overlap between each current type and each positive test
alternative.

## Why

`is_numeric($text)` on a known string yields a numeric string.
The test's integer and float alternatives cannot replace that string.
