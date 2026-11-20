# IntlDateFormatter reads DateTimeInterface and DateTimeZone through Phera's date state

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Design

## Decision

IntlDateFormatter reads DateTimeInterface and DateTimeZone through Phera's date state.
NumberFormatter uses fixed_decimal and caches a bounded number of ICU4X currency formatters.

## Why

Reuse the timezone database and avoid constructing a currency formatter for every call.

## Turned down

A second timezone implementation, PHP callbacks for each date and an unbounded currency
cache.
