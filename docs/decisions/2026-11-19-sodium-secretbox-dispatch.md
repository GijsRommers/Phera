# Preserve portable builds and runtime dispatch for secretbox

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Design

## Decision

Use Salsa20 0.10.2 and Poly1305 0.8.0 through crypto_secretbox. Salsa20 uses a portable
backend. Poly1305 uses a portable backend on Apple silicon and detects AVX2 at runtime on x86.

## Why

The same build can run on supported CPUs with and without AVX2. The hosted x86 runner selected
AVX2.

## Turned down

Forcing CPU target flags for the extension.
