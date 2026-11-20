# Send string echo through the output writer

- Date: 2026-11-20
- Area: Runtime
- Decided by: Design

## Decision

Send a string operand directly to the existing output writer. Flush any
pending operator diagnostics before writing. Other operands use the
existing conversion.

## Why

A plain string needs no conversion or float precision. Using the existing
writer keeps output buffers, callback scheduling, and headers in the same
order.

## Turned down

Specializing the interpreter loop or maintaining a second writer for
buffered strings.
