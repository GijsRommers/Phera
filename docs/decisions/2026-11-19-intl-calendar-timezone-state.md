# Intl calendar and time zone objects reuse PHP date instants and offsets

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Design

## Decision

Intl calendar and time zone objects reuse PHP date instants and offsets. ICU4X supplies
calendar and week data, localized zone names and mappings. `formatObject` keeps the calendar
object's type and zone. Windows zone mappings fall back to region 001 and use PHP's legacy
canonical aliases.

## Why

DateTime conversions and date formatting use the same transition rules. ICU4C's Windows
lookup returns a world mapping when a regional mapping is absent.

## Turned down

A separate offset database for intl, implicitly switching a calendar object to the display
locale's calendar, or failing Windows lookups for missing regional overrides.
