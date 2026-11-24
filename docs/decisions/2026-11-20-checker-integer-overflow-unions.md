# Keep arithmetic overflow separate from a declared numeric union

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Represent arithmetic that can overflow an integer separately from a
declared `int|float`. Treat the overflow alternative as insufficient proof
of an error when passing the result to an integer parameter.

## Why

PHP can promote arithmetic to float on overflow. PHPStan does not reject
every integer addition used later as an integer.

## Turned down

Applying the declared-union rule to overflow alone or dropping the
possibility of a float result.
