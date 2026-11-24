# Apply inline variable annotations to return expressions

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Apply an unnamed `@var` immediately before a return statement to the
returned value. Evaluate the expression and its own diagnostics before
checking the annotated value against the return contract.

## Why

PHPStan accepts this form for values whose type is known at the call site.
A query result may have a known row shape even when its method returns
`mixed`.
