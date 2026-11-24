# Use scalar constants in strict membership guards

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Use indexed scalar constants when refining the subject of
`in_array($value, [ClassName::CONSTANT], true)`. The false branch can
subtract a member only when its exact value is known.

## Why

A successful strict comparison establishes the subject's type even if
the constant's particular value is unavailable.
