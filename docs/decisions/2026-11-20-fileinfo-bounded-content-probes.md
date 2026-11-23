# Identify uploads with bounded Rust content probes

- Date: 2026-11-20
- Area: Safety and dependencies
- Decided by: Design

## Decision

Fileinfo uses checked Rust header probes and text recognition for common file
formats. It reads up to 7 MiB from a named stream and inspects up to 64 KiB for
encoding, following PHP's file and encoding limits. It reads a four-byte
trailer for the file form of the gzip description. The JSON probe reuses
Phera's JSON parser with a bounded depth and leaves JSON error state alone.

## Why

Fileinfo reads untrusted upload bytes. Common PHP application formats can be
identified from bounded headers and text, with tests against PHP's bundled
libmagic database. MIME type guesses do not depend on a supplied filename.

## Turned down

A C dependency on libmagic, and a runtime parser for libmagic databases.
