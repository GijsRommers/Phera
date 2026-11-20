# `basename()` is an ordinary built-in function

- Date: 2026-11-20
- Area: Engine and standard library
- Decided by: Design

## Decision

`basename()` is registered next to `dirname()` and `pathinfo()` in the
streams extension (`crates/extensions/streams/src/path_text.rs`) and takes
its arguments through the same check as every other built-in function. The
special case for it inside the VM is removed.

## Why

The special case dated from before the standard library existed. It turned
its arguments into strings itself, so an object with `__toString()` (a
`SplFileInfo`, which Symfony's cache warm-up passes) was refused with
"could not be converted to string", and a wrong type gave an `Error` where
PHP throws a `TypeError`. The shared argument check already does all of
this correctly. The benchmarks and the cost of a Laravel request did not
change.

## Turned down

Teaching the special case to convert objects: a second copy of the argument
rules, to be kept equal to the first by hand.
