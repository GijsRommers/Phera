# Inline small-slot object storage and in-loop hot operations

- Date: 2026-11-23
- Area: Speed and method
- Decided by: Design; the owner can overrule it

## Decision

1. Store up to four declared object properties inline inside `phera_value::Properties` (`PropertySlots::Inline`) and skip `PhpArray::release_entries` on drop when no slot holds an `Array`, `Object`, or `Reference`.
2. Perform integer property increments (`$this->count++`) in a single `RefCell` mutable borrow (`property_storage::increment_known`).
3. Fast-path `Class::derives_from` (pointer equality, leaf class, direct parent, direct interfaces) before allocating the traversal stack, and resolve loaded classes in `fast_instance_of` via `ClassTable::by_key` without cloning `Rc<Class>`.
4. Handle `self::`/`static::`/`parent::`, private/protected class constants, validated backed enums, and `FetchClassConstant` (`$obj::CONST`, `$obj::class`, `$class::CONST`) inside `Machine::run`.
5. Execute `ArraySet`, `ArrayPush`, `ArrayKeyExists`, `InLiteralArray`, integer string offsets, array destructuring reads, and identity `Cast` inside `Machine::run`.
6. Avoid cloning `Rc<Class>` in `InitStaticCall`, execute public user `StaticCall` in the loop, and avoid allocating class/method `String`s in `static_style_call`.
7. Fast-path validated backed-enum `from()`/`tryFrom()` inline in the loop and in `Vm::call_enum_method`.
8. Execute pure, side-effect-free synchronous `CallNativeDirect` built-ins in `Machine::run` (`PureNative`) and dispatch non-pure `CallNativeDirect` directly in `run_batch_loop`.

Single-byte interned strings for string offsets and `chr(0..=255)` are deliberately deferred to the one-variant `PhpString` work so this PR stays independent of string representation changes.

## Why

Settled Symfony/Doctrine and Laravel profiles showed repeated heap allocation for 1–4 property objects, double `RefCell` borrows on property increments, stack zeroing and `Rc<Class>` clones on `instanceof`, and thousands of interpreter-loop exits per request for class constants, array literals, `StaticCall`, backed-enum `from()`/`tryFrom()`, and pure string/array `CallNativeDirect` built-ins. Each improvement keeps 100% safe Rust (`#![forbid(unsafe_code)]`), introduces no new lookup caches, and is isolated in its own commit so every item can be gated independently on `main`.

## Turned down

- Stacking on a two-variant `Value::InternedStr` representation (adds a second arm to every string match across the workspace; one-variant `PhpString` handles interning cleanly inside `crates/value`).
- Bundling all eight items into one commit (individual commits let the gate keep only the changes that pay on every route).
