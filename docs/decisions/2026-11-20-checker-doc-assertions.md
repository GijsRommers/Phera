# Apply unconditional receiver assertions after instance calls

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Apply a resolved method's positive `@phpstan-assert` or `@psalm-assert`
to named places on the normal path after an ordinary instance call.

## Why

A method can establish that a nullable property is initialized. The caller
can use the assertion without rechecking the property.

## Turned down

Assuming a nullable property always holds an object after a call, or
applying assertions when the call may have been skipped by nullsafe access.
