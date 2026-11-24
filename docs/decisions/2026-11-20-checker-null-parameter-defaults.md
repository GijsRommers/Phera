# Keep null parameter defaults when applying PHPDoc

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Include null in the type of a parameter with a null default when its PHPDoc
type describes the arguments callers supply.

## Why

A parameter such as `/** @param string $name */ function f($name = null)`
can be null when the argument was omitted. Its body needs the null case.

## Turned down

Using only the PHPDoc type once it is available.
