# Explicitly excluded intl classes return named unsupported errors from construction and static entry points

- Date: 2026-11-19
- Area: Safety and dependencies
- Decided by: Design

## Decision

Explicitly excluded intl classes return named unsupported errors from construction and
static entry points.

## Why

A caller can distinguish the excluded operation from a general runtime lookup failure.

## Turned down

Returning empty transliteration or converter results.
