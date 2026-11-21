# `class_exists` and its relatives answer in the interpreter loop when no PHP code can change the answer

- Date: 2026-11-21
- Area: Calls
- Decided by: Mistral Large 4 (implementation), reviewed by Fable

## Decision

`class_exists`, `interface_exists`, `trait_exists` and `enum_exists` are answered inside the batch loop, the way `spl_object_id` is, when the call is plain positional with one or two arguments, the first is already a string, and the second, when present, is already a bool (`crates/vm/src/class_queries.rs`; the routing arms sit beside the `ObjectId` arm in `crates/vm/src/calls/sync_native.rs`):

- the name is looked up in the request's class table with `ClassTable::get`, which strips a leading backslash and lowercases into a stack buffer without allocating; the answer is the entry's kind;
- a miss with the autoload flag off answers `false`;
- a miss with the flag on answers `false` only when no autoloader is registered (the same `autoload.callbacks` state the general lookup reads) and the name is not one a bundled source can have (`bundled_classes::is_bundled_class_name`, the predicate `load_bundled_class` itself uses); otherwise the call falls back.

Every other shape — a named or coerced argument, a name that is not valid UTF-8, a wrong argument count — returns `BatchExit::NativeCall` with the window intact, and `Vm::class_intrinsic` remains the single definition of the full behaviour, including running the autoloader and raising PHP's errors.

## Why

Before, one asynchronous handler served the whole family: every call left the loop as a boxed future with a `Vec` of arguments and a `String` copy of the name per call, even when the class was already loaded and the answer was one table lookup (`class_exists` cost 5,216 instructions per call where PHP needs 98). After, the general handler still serves every general case, and the loop answers the shapes listed with no allocation: `class_exists` now costs 2,492, at the floor of the existing in-loop `spl_object_id` (2,727), whose remaining cost is the shared call preparation, not the query.

## Turned down

Routing these calls through the lean native window (`CallNativeDirect`) like `strlen`: intrinsics are general callees with a details record, and changing that machinery is a different change in `calls/`. Caching answers: the class table is the source of truth, and a class can be declared later in the request.
