# Validate control syntax at its source

- Date: 2026-11-21
- Area: Parser
- Decided by: Design

## Decision

Use the current expression precedence to reject an ambiguous ternary
chain. Parentheses and the middle operand have their own expression
loops. Consecutive short ternaries can still group from the left.

Validate loop operands before lowering them to a depth. Validate the
first namespace against the parsed file prelude, where leading declare
statements can be distinguished from other statements.

## Why

The parser already knows which operators shared a precedence level and
whether loop depth came from an integer literal. Keeping those facts
here avoids extending every expression node or adding a runtime check.

## Turned down

Accepting an invalid loop operand as depth one or deciding how to group
an ambiguous ternary.
