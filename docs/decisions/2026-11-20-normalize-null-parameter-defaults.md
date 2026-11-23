# Normalize literal null defaults during function compilation

- Date: 2026-11-20
- Area: Compiler
- Decided by: Design

## Decision

When an ordinary non-nullable parameter has a literal `null` default,
give its compiled declaration a nullable type and emit the PHP deprecation.
The default prologue, argument checks and reflection use that declared type.

## Why

Both explicit and omitted arguments can supply null to the same parameter.
The ordinary nullable type already describes both cases. A promoted property
has its own declaration, so a null default requires a nullable annotation.

## Turned down

Special-casing default values during every argument check and reconstructing
nullability when reflection reads the parameter.
