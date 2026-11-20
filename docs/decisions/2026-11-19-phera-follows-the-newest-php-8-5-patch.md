# Phera follows the newest PHP 8.5 patch release

- Date: 2026-11-19
- Area: Scope and compatibility
- Decided by: Owner

## Decision

Phera follows the newest PHP 8.5 patch release. A comparison whose PHP answer changed in a patch release is marked with the version it holds from and is skipped, visibly, on an older PHP. It is never removed.

## Why

Patch releases are bug fixes; CI moves to them on its own; nobody runs an old patch on purpose.

## Turned down

Pinning CI to one patch version.
