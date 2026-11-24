# Check unused alternatives on initializer-only private properties

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

At level 4, check private properties with a known literal initializer and
no assignment in the declaring class. Report union alternatives ruled out
by the initializer. Skip the rule when an enabled framework extension
can write the property. Doctrine mapping effects use the shared attribute
table, so core and extension-enabled checks follow their references.

## Why

A property whose only value is its initializer needs no unrelated
type alternative. ORM hydration can populate that alternative even
when there is no assignment in PHP source.

## Turned down

Guessing the types of writes through unknown receivers or references.
