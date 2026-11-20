# Native classes receive validated legacy payloads

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

A linked class can register a native legacy `C:` deserializer. The wire
decoder resolves the class, validates the payload length, then allocates
and registers the object before passing the payload bytes to the handler.
SPL's explicit `unserialize()` methods and the wire hook use the same
class-specific decoders.

## Why

A native payload can restore its state without publishing a synthetic PHP
method or implementing Serializable. The decoder retains the allowlist,
depth limit and outer object-reference slots.

## Turned down

Inferring a native deserializer from a class name or requiring the extension
to parse the surrounding wire record.
