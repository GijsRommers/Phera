# RustSec flags `chrono` as unmaintained (RUSTSEC-2026-0543)

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Owner

## Decision

RustSec flags `chrono` as unmaintained (RUSTSEC-2026-0543). Replace it with a maintained date API, then remove the advisory exception once `chrono` leaves the lockfile.

## Why

Preserve PostgreSQL date and timestamp decoding, HTTP cookie expiry formatting, and session cookie and cache-header dates during the migration.

## Turned down

Combining the date-compatibility migration with the TLS provider change.
