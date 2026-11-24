# Use indexed literals for named class constants

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Use the indexed literal on an exact named class constant access.
Dynamic class names and late static access keep their declared type.

## Why

Bitwise flags such as `Mode::OPTIONAL | Mode::MULTIPLE` have an integer
result when both constants are known integers.
