# `self::CONST` and `parent::CONST` in a method compile to the named form

- Date: 2026-11-21
- Area: Compiler, expression lowering
- Decided by: Mistral Large 4 (implementation), reviewed by Fable

## Decision

- `self::K` and `parent::K` in a method body of a named, non-trait class compile to `NamedClassConstant` with the resolved class name (`self` is the class being compiled, `parent` its resolved `extends`), exactly as `C::K` does. The constant is then read by the class's symbol key, with the ready path in the loop, instead of a `ClassName` register resolved by name on every read.
- Everything else keeps the general register form (`ClassConstant`): `static::` (the called class is known only at run time), traits (`self` is the class that uses the trait), closures and arrow functions (`self` can be rebound with `Closure::bind`), anonymous classes, constant and property initializers (the self-referencing-constant message keeps the written `self::`, which the named form cannot carry through lowering), and `parent::` in a class without a parent, an interface or an enum (where PHP raises its error at run time today).
- The compiler learns the parent from a new `parent_scope` field, set in `compile_class` from the same resolved `extends` the `ClassDeclaration` records, and carried into method builders in `compile_captured_function`.

## Why

A constant read through `self` cost about 1,200 instructions more than the same read through the class's name (2,534 vs 1,329 per call on main `326e7438`, PHP 148), because the general form resolves the class register by name at run time on every read. After the change `self::K` costs the named form's 1,326. Error texts do not change: in a method body they are built from the resolved or declaring class name, which is the same for both forms, and the one spelling-dependent message (self-referencing constant) only arises in initializers, which keep the general form.

## Turned down

- Specializing initializers too. The named form's symbol carries one string, and lowering turns it into both the lookup key and the written spelling, so a self-referencing `self::K` would print the resolved class instead of `self::`. Initializers run once per request at link time, so there is no per-call cost to recover.
- A written-spelling field on the named instruction. That would touch `passes/lowering.rs`, which another engineer owns this week.
