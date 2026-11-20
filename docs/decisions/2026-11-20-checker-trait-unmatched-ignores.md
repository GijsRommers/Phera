# Leave unmatched ignores in trait bodies to trait consumers

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Continue to honor inline ignore directives in traits. When reporting
unmatched directives, skip those in a trait body.

## Why

Different uses of the same trait can change whether an ignored
condition is true. A diagnostic pass over the trait's source alone
cannot establish that the directive never matched.

## Turned down

Treating the lack of a local trait diagnostic as an unused directive.
