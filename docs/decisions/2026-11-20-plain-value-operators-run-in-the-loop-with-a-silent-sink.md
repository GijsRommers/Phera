# Plain value operators run in the loop with a sink that notes any diagnostic

- Date: 2026-11-20
- Area: Speed and method
- Decided by: Design

## Decision

A binary or unary operator whose operands are all null, booleans, integers,
floats or strings runs inside the loop through the value layer's own
operator (`crate::ops::binary`, `unary`), with a diagnostics sink that only
records whether anything was raised or the `precision` setting was asked
for. If so, or if the operator failed, the result is dropped before
anything is written and the VM runs the instruction again with full
diagnostics. The same module reads array elements with an integer or
string key, quietly where the mode is quiet. (`crates/vm/src/fast_values.rs`)

## Why

These are the most frequent instructions that left the loop in framework
code (string comparison and concatenation, `!`, `??`). Using the value
layer's operator keeps one definition of PHP's rules; running it twice in
the rare case is safe because on these operands it has no effect.

## Turned down

Hand-written fast cases per operator and type pair (a second copy of PHP's
conversion rules to keep right); passing the VM's own diagnostics into the
loop (the loop does not borrow the VM).
