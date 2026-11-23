# A ReflectionMethod made from a class's member list addresses its method by position

- Date: 2026-11-21
- Area: Reflection
- Decided by: Design

## Decision

A class's member list gives each method a `MethodHandle { class, index }`: the declaring class's number and the method's position in its method table.

A `ReflectionMethod` made by `getMethods()` keeps that handle (`FunctionTarget::Member`). Its `FunctionInfo` comes from a column per class in the reflection cache (`method_functions`), filled on first use, instead of being found again by name.

## Why

Every question to a `ReflectionMethod` lowercased its name, hashed it into the method table and walked to the declaring class. `getMethods()` of ten methods: 46,128 → 39,057 machine instructions (PHP 5,330).

## Turned down

A compiler pass that evaluates Reflection calls ahead of time: the common answers are data the class already has.
