# Resolve conditional class names during variance checking

- Date: 2026-11-21
- Area: Class linking
- Decided by: Design

## Decision

Check inheritance using linked definitions, the declaration being
checked and top-level declarations eligible for early binding. If a
method contract needs a class declared in an unexecuted branch, defer
the check to the existing class lookup and autoload path.

For constructors, keep an abstract ancestor's signature as a contract
even after an intermediate class supplies a concrete implementation.

## Why

A conditional declaration contributes a class only after its branch
runs. An autoloader can choose whether to reach that declaration.
Keeping the lookup in the existing path preserves the observable loader
calls and leaves provisional inheritance cycles to the linker.

## Turned down

Inferring ancestry from all class declarations in the compiled file.
