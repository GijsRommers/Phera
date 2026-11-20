# Declare PHP parameters for unsupported intl methods

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

Use the PHP stub's parameter declarations for every registered intl method and
function. Read optional defaults from the generated catalog where the binding
does not need to supply its own value. Unsupported method bodies keep throwing
the named exception after native argument validation.

## Why

Reflection creates parameters from the registration and reads their defaults
from the catalog. Named calls and argument counts depend on the same declared
parameters even when the method body is unsupported.

## Turned down

A variadic placeholder for unsupported methods or a null default where PHP does
not provide one.
