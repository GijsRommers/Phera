# Update borrowed integer locals directly

- Date: 2026-11-20
- Area: Speed and method
- Decided by: Design

## Decision

Compound assignment and increment update the integer payload through the
register borrow that checked its type. Read a compound assignment's right
operand first so `$x += $x` uses the old value on both sides.

## Why

The merged fat-LTO build retired more instructions on arithmetic and
integer loops despite generating identical PHP bytecode. Both instructions
already know that the target is an integer. Updating that payload once
avoids repeating register and type checks while keeping the existing
fallback for overflow and coercion.

## Turned down

Changing the bytecode or adjusting interpreter inlining hints to recover
the benchmark's previous code layout.
