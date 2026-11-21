# Resolve conditional variance types at declaration

- Date: 2026-11-21
- Area: Class linking
- Decided by: Design
- Status: Superseded by [Publish classes after recursive variance checks](2026-11-21-publish-classes-after-recursive-variance.md).

## Decision

During a class declaration outside an active autoload, derive variance
from linked classes, the owners being checked and declarations eligible
for early binding. Ask the autoloader to resolve other class names.
Keep the existing lookup for nested autoloads.

## Why

A class declaration hidden in an unexecuted branch supplies no ancestry.
PHP code can observe the autoloader being called for that name. Nested
autoloads also need a way to see class shapes with unfinished variance
checks. That requires a separate change to the linker.

## Turned down

Removing conditional source declarations from all checks at once.
That caused regressions in three recursive autoload tests.
