# Curl verification defaults and per-handle overrides

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Owner

## Decision

Curl verifies the server's certificate chain and hostname by default. A handle
can explicitly turn either check off using PHP's curl options. TLS handshake
signatures and configured public-key pins still apply. The verbose trace names
each disabled check, and the worker keeps separate pools for different
verification settings.

## Why

PHP applications use these options with self-signed development endpoints.
Separate pools keep connections made under one verification policy from being
reused by handles with a different policy.

## Turned down

Refusing the options or relaxing verification by default.
