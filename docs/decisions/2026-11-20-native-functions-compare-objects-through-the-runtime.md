# Native functions compare objects through the runtime

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

Use the request comparison for object operands, including comparisons in
nested arrays. A scalar positional `min()` or `max()` call can still select
the synchronous handler. Variadic comparisons put the candidate on the
left. Array comparisons put the current winner on the left.

## Why

The request comparison has the linked native operators and can run PHP
string conversion. A comparison inside an array needs the same hooks as a
comparison at its root. Plain numeric values keep the dedicated path.

## Turned down

Adding the engine's own object ordering rules to the standard library.
The two-argument frameless forms in PHP have separate tie and operand-order
behavior. Phera uses the regular variadic contract for object operands.
