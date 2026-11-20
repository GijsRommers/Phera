# Record framework member usage in an attribute table

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Use fully qualified attribute names to determine whether Doctrine or
Symfony reads, writes, or invokes a member. Resolve aliases through the
same name lookup used for PHP expressions.

## Why

Frameworks act on attributes through reflection. Exact attribute names
give the checker a concrete reason to account for an external use.

## Turned down

Assuming every attribute under the Doctrine mapping namespace means a
property is read and written. The comparison targets use attribute
mappings. Loading XML and legacy annotation metadata is outside this
change.
