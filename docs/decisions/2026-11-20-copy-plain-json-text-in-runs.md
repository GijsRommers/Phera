# Copy plain JSON text in runs

- Date: 2026-11-20
- Area: Extensions
- Decided by: Design

## Decision

Copy plain ASCII runs directly into an encoded JSON string. Route bytes
that might need escaping or UTF-8 handling through the existing encoder.

## Why

Keys and ordinary text often contain several consecutive ASCII bytes.
Appending a run avoids passing each byte through the UTF-8 and escape
handling separately.

## Turned down

Skipping the exceptional-byte check for all ASCII. Control bytes and
flag-dependent escapes can still occur in ASCII inputs.
