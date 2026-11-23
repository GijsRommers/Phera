# A native method without parameters that only reads its object runs at its InitMethodCall

- Date: 2026-11-21
- Area: Calls
- Decided by: Design

## Decision

A native method may have a direct handler, `fn(&Object, &[Option<Value>]) -> Option<Value>`. The handler reads the object and the arguments, and answers `None` when the general path must run.

When the loop prepares a call of such a method with no parameters and the next instruction is that call's `Call`, it runs the handler there, puts the value in the call's destination and skips the `Call`: no call record and no frame.

Reflection has direct handlers for:
- `getName`, `isClosure`, `isDeprecated`, `isInternal`, `isUserDefined`, `isGenerator`, `isStatic`, `returnsReference` and `hasReturnType`;
- `getAttributes()` of a member without attributes;
- `getReturnType()` of a member without a return type.

## Why

`ReflectionMethod::getName` cost 2,748 machine instructions (PHP 240), now 770; `isStatic` 2,684 → 792. These are flag and name reads the class already holds.

## Turned down

Running every native method inside the loop: handlers that take the whole VM need its state back, which costs every other instruction.
