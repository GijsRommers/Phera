# Serialize bounded, copyable Sodium hash state

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Keep the BLAKE2b chaining words, a 128-bit compressed byte count and one pending block in a
checksummed 384-byte PHP string. Validate each state before use. Preserve the full chaining
result for PHP’s independent init and final digest lengths.

## Why

Copying the opaque PHP string creates an independent hash state. A bounded block buffer keeps
memory use independent of the message length.

## Turned down

A process-local handle or a state string containing the full input.
