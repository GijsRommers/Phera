# Track guarded self static properties

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Track named `self::$property` paths within methods. Discard the
facts after calls that may mutate the properties. When every
branch leaves the same fact, keep its type and drop the source
location if the branches disagree on that location.

## Why

A static property initialized in a null guard can be used immediately
after the guard. Another call can reassign the same property.

## Turned down

Keeping refinements for dynamic or late-bound static properties.
