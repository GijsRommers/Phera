# Run diagnostic handlers before native output

- Date: 2026-11-21
- Area: Scope and compatibility
- Decided by: Design

## Decision

When a native converts values and writes the resulting bytes, settle
pending conversion diagnostics before its first write. The VM supplies a
future only when there is a handler to deliver. Keep the rendered bytes
when a handler throws an exception or calls `exit()`. A fatal diagnostic
stops the write.

## Why

PHP lets a warning handler emit its own output before the bytes from
`printf`, with or without output buffering. The handler may call arbitrary
PHP code and suspend, so this takes an async hook in the writing native.
Scalar formats that cannot warn can still use the synchronous path.

## Turned down

Draining the handler from the synchronous output sink would need to run
PHP code inside a call that cannot suspend. Doing it at every native call
would charge functions that never write anything.
