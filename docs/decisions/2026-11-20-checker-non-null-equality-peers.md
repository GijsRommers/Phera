# Use non-null values in equality guards

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

A strict comparison that establishes equality with a stable non-null
value also establishes that the other value is non-null.

## Why

On the equal path of `$name === $record->name`, a native string property
rules out null for the local variable.
