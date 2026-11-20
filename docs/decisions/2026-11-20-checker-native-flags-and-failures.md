# Fill known native flag values and literal failure returns

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Use verified native flag values and literal `false` returns to fill
gaps in the stub inventory. Evaluate integer bitwise operators when
both operands are known integers.

## Why

PHP stubs do not encode all constant values or distinguish `false`
from `bool` in every fallible function. The distinction matters for
flag-dependent returns and for guards such as `!== false`.
This also applies to color allocation, stream reads, and OpenSSL
decryption.

## Turned down

Guessing the value of an unknown constant or folding bitwise
operators with uncertain operand types.
