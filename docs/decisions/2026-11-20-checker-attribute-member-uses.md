# Count references inside PHP attributes as member uses

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Walk attribute arguments when deciding whether a private member is used.

## Why

Attribute arguments can depend on private constants, for example
`#[EntityTag(self::TAG)]`. PHP evaluates these arguments when attributes
are instantiated through reflection.

## Turned down

Treating attributes as declaration names only in the member usage walk.
