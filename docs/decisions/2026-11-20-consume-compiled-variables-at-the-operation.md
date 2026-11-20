# Consume compiled variables at the operation

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

Keep named variable operands in their registers until an eager binary
operator consumes them. Array literal keys and sequential match subjects
use the same rule. Array dimensions, property access and other value
expressions still fetch where they occur. Short-circuit branches and
argument sends consume their operands at their existing boundaries.

## Why

Zend keeps an ordinary local, static local or local bound by `global`
as a compiled variable operand. If the right expression changes it,
the operator uses its current value. A warning from an undefined
operand can change another operand through an error handler. The
interpreter handles that case when it sees an empty operand register.

## Turned down

Taking a snapshot for every variable expression, or evaluating the
right expression before resolving all forms of the left expression.
