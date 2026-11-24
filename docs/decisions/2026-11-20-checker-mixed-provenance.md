# Track where mixed types come from

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Distinguish an explicit mixed declaration, an implicit mixed declaration,
and a fact the analyzer could not infer. Enable mixed diagnostics at levels
9 and 10 for the declared cases PHPStan checks at those levels.

## Why

Treating an unsupported analysis path as a declaration of mixed produces
findings on code with a type contract the analyzer does not yet understand.

## Turned down

Applying strict mixed checks to every unknown result.
