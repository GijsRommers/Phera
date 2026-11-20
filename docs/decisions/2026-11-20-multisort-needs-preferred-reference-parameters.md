# Multisort needs preferred-reference parameters

Replaced by [Multisort binds writable arguments and accepts temporaries](2026-11-20-multisort-binds-writable-arguments-and-accepts-temporaries.md).

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

Keep `array_multisort()` outside this comparison change.

## Why

PHP's signature prefers references so it can update an array variable and
also accept literal sort flags. The current binder requires a writable
variable for a by-reference parameter. Correct multisort support needs
both preferred-reference arguments and a sort that can update multiple
arrays together.

## Turned down

Using required-reference parameters and rejecting literal sort flags.
