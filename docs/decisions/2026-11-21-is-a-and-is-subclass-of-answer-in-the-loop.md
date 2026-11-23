# `is_a` and `is_subclass_of` answer in the interpreter loop with the general handler's own class test

- Date: 2026-11-21
- Area: Calls
- Decided by: Mistral Large 4 (implementation), reviewed by Fable

## Decision

`is_a` and `is_subclass_of` join the `*_exists` functions in `crates/vm/src/class_queries.rs` for a plain positional call (two or three arguments: an object or string first, a string second, an optional bool `allow_string`):

- an object first argument uses its class (`ClassTable::for_object`); the second argument is looked up in the table only, because PHP never autoloads it;
- a string first argument with `allow_string` off names no class, even one that is loaded; with it on, the table answers when it has the name, and a miss falls back when a loader is registered or the name can be bundled, otherwise answers `false`;
- the answer is the general handler's own test, `Class::is_a(&target.name)` against the class's precomputed lowercase ancestry set, plus the index comparison that makes `is_subclass_of` refuse the class itself.

The class test is a name test, not the pointer walk `instanceof` uses in the loop: that walk (`derives_from` in `calls/fast_instance_of.rs`) is private to `calls/`. When `Class::derives_from` lands on main, this module switches to it in a commit of its own, with the micro numbers before and after.

## Why

These calls left the loop for the same boxed async handler as the `*_exists` family (6.6k and 8.3k instructions per call where PHP needs 250-300). They now cost 3.7k and 5.0k; the part above the `*_exists` floor (2.5k) is the ancestry set lookup, which allocates for a target name with uppercase letters, exactly as the general handler does.

## Turned down

Reusing `instanceof`'s pointer walk from this module: it would mean widening a `calls/` item's visibility for one caller, and the walk's fixed stack has to fall back on deep hierarchies. Waiting for `Class::derives_from` instead.
