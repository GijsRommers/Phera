# Omit arguments from Sodium exception traces

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Design

## Decision

Register native metadata for empty trace arguments in SodiumException and the parsing errors
that PHP redacts.

## Why

Capturing a caller’s frame arguments can expose secrets that were passed through the caller.

## Turned down

Redacting only the arguments declared sensitive on the Sodium function itself.
