# Use inherited PHPDoc inside untyped overrides

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

At level 10, walk the indexed ancestor contracts for parameters that
still have implicit mixed inside a method. Use a parameter's inherited
PHPDoc by position and apply known receiver type arguments.

## Why

An override can omit a PHPDoc tag or rename the parameter while
retaining the inherited contract. The body can use what the contract
already establishes.

## Turned down

Inheriting from a private ancestor method or overriding a parameter
type already supplied by the method's own declaration.
