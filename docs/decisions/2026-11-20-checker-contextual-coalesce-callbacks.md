# Type array_map callbacks for configured coalesce checks

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Use the input element type of `array_map` to check callback parameters when
the project has enabled `unnecessaryNullCoalesce`. Use the same inference
as the upper levels.

## Why

A required nullable key stays present when the row is passed into a callback.
The optional PHPStan rule starts at level 1.

## Turned down

Re-evaluating the array argument or treating all callback parameters as
known shapes.
