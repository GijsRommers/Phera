# Phera verifies intl types, references, diagnostics and locale-independent results exactly against PHP 8.5

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Design

## Decision

Phera verifies intl types, references, diagnostics and locale-independent results exactly
against PHP 8.5. It checks the core locale data on the oracle host and records each accepted
difference with both CLDR releases.

## Why

ICU4X and PHP's ICU4C can use different locale data even on the same PHP patch release.

## Turned down

Pinning all locale output to one machine.
