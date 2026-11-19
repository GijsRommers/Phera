# The [Advisories job](../.github/workflows/ci.yml) scans the whole lockfile on pull requests and on `main`

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Owner

## Decision

The [Advisories job](../.github/workflows/ci.yml) scans the whole lockfile on pull requests and on `main`. The [ignore file](../.github/advisory-ignores.json) requires a reason for every exception.

## Why

New advisories fail the check. An existing exception stays visible until the dependency can be replaced.

## Turned down

Warning-only scans or a blanket exemption for maintenance advisories.
