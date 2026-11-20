# Hydration constructors run in their declaring scope

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

Native extensions call the constructor of an already allocated object
through a context operation. It routes positional and named arguments,
and runs the constructor in its declaring scope. PDO row and statement
hydration use the operation.

## Why

A database factory sets state or columns before entering PHP. PHP permits
the factory to invoke private and protected constructors without making
them callable from script scope.

## Turned down

Invoking the public `__construct` method from the factory's caller scope,
or routing the invocation through the Reflection extension.
