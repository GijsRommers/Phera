# Apply conditional PHPDoc assertions on their declared branch

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Apply PHPStan and Psalm conditional assertions when the result of a named
call selects the asserted branch. Bind assertions to the actual argument
or receiver property. A negated assertion removes the stated type.

## Why

A predicate can promise an argument is non-null when it returns true.
The next statement in that branch can use the promised type.
