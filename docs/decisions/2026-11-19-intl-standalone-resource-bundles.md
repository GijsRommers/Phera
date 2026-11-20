# ResourceBundle exposes selected CLDR tables and reads native-endian standalone ResB directories asynchronously

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Design

## Decision

ResourceBundle exposes selected CLDR tables and reads native-endian standalone ResB
directories asynchronously. The decoder walks resource references with explicit bounds.

## Why

ICU4X supplies typed locale data and applications can provide independent bundle files.
Bounded decoding handles corrupt files and cyclic offsets.

## Turned down

Linking the ICU4C resource loader or recursively following file-controlled offsets.
