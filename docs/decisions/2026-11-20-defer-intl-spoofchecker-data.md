# Keep Spoofchecker unsupported pending confusables data

- Date: 2026-11-20
- Area: intl security checks
- Decided by: Design

## Decision

Keep the explicit Spoofchecker exception with the current ICU4X dependency.

## Why

The pinned ICU4X components lack the confusables data and checker used by
Symfony's NoSuspiciousCharacters validator.

## Turned down

Using a script-count heuristic for a PHP API that promises Unicode spoof
checks.
