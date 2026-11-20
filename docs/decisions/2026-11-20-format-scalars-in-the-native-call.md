# Format scalars in the native call

- Date: 2026-11-20
- Area: Extensions
- Decided by: Design

## Decision

Use the synchronous selector for scalar formatted output and joins that
cannot run PHP code. A single format parser writes fields into the result and
hands control back to the async caller when a string field needs an object
conversion. Borrow existing string bytes through padding. Measure the joined
output before allocating when the inputs have known conversion lengths.

## Why

Joins no longer allocate a future per plain value or an intermediate PHP
string per integer on the synchronous path. Formatted output uses the same
parser and conversions on both paths, including when an earlier specifier
has emitted a diagnostic. Object string conversions still run at the same
point in the format evaluation.

## Turned down

Precomputing every formatted field's size by converting the arguments twice.
A conversion may call PHP or emit diagnostics. Fields with a width or a
converted length larger than the initial format buffer can grow the output
as they are written.
