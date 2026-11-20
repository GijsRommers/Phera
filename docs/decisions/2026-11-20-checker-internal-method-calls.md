# Check calls outside the root namespace of internal methods and classes

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

At level 2 and above, with the PHPStan internal-tag feature enabled,
check method calls against the declaring class or method's
`@internal` tag and root namespace.

## Why

An internal API can change without a public compatibility guarantee.
The project can opt into the same method checks as PHPStan.

## Turned down

Applying the bleeding-edge rule to projects using only core rules
or treating a nested namespace in the same library as external.
