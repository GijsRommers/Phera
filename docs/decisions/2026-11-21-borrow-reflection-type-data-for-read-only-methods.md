# Borrow reflection type data for read-only methods

- Date: 2026-11-21
- Area: Speed and method
- Decided by: Design

## Decision

Inspect native reflector data in place when returning a type name or
checking a type flag. A reader takes a reference to the existing
payload and completes before the borrow ends. Operations that pass the
payload onward still take an owned copy.

## Why

The Doctrine enum probe calls getBackingType and getName together. A
name read can create its PHP result from the stored declaration in one
pass. The backed-enum queries can read the declared backing kind
without taking ownership of the class reflector.

## Turned down

A shared ReflectionNamedType would change PHP-visible object identity.
An extra reference-counted string inside the reflector would increase
the cost of constructing a reflector for one query.
