# Join compatible facts about the same place across branches

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

When all continuing branches have a fact for a place, keep the union
of the known types. Drop the source position if the branch facts
disagree. Let the declared type apply if the joined fact is unknown.

## Why

After testing an offset with `is_int($row['count']) ||
is_float($row['count'])`, the true branch knows the offset is numeric
even though the two alternatives established different types.

## Turned down

Keeping the fact from just one branch or treating different source
positions as evidence that the place no longer has a known type.
