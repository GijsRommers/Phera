# Scan the complete ISO/IEC 7816-4 padding block

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Design

## Decision

Use block-padding to encode ISO/IEC 7816-4. Use subtle to scan the complete final block during
unpadding.

## Why

Unpadding needs the PHP length rules and must avoid early exit based on the padding bytes.

## Turned down

The block-padding decoder, which exits early while reading the padding.
