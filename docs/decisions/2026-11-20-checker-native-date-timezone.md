# Use the timezone of an initialized native date

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Use `DateTimeZone` for the normal result of the native
`DateTimeInterface::getTimezone()` methods.

## Why

PHP 8.5 returns a timezone for constructed dates and throws
`DateObjectError` for an uninitialized date. PHPStan types the normal
result as a timezone.
