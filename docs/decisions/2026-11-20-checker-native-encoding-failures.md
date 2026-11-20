# Refine JSON and Base64 result types from their flags

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Treat the failure alternative of JSON encoding and Base64 decoding as
literal false. Use string as the return type for non-strict Base64
decoding and JSON encoding with the known exception flag.

## Why

PHP never returns true here. In these flag modes it returns a string
or throws, so a caller can use the result as a string.

## Turned down

Assuming that an unknown flag sets the corresponding native option.
