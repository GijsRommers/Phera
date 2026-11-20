# Keep loop-only definitions uncertain with strict rules enabled

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

When strict rules are enabled, definitions introduced only by a
for-loop initializer or foreach iteration are uncertain afterward.

## Why

The strict-rules package disables propagation of these definitions.
A later foreach should report an overwrite only when the variable was
already defined independently of those loop-local assignments.

## Turned down

Changing the definition rules for projects using core PHPStan.
