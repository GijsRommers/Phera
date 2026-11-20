# Leave Doctrine query result types open without query metadata

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

For a known Doctrine ORM or DBAL query-result method, use an
unresolved type when its declaration gives only mixed. Keep concrete
return types that can be recovered from the declaration.

## Why

The final result type can depend on query projections, mapping, and
database conversion. PHPStan's Doctrine extension can know more
than the generic method signature provides.

## Turned down

Assuming that every scalar projection returns a fixed PHP type or
weakening the mixed rule for all methods.
