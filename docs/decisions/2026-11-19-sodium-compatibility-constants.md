# Report the Sodium compatibility constants

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Use the Sodium 1.0.22 constants observed with the PHP oracle, including the library-version
constants and AEGIS constants.

## Why

Existing PHP code reads these constants. They describe the compatibility surface of the Rust
extension.

## Turned down

Inventing a runtime library version independent of PHP’s Sodium constants.
