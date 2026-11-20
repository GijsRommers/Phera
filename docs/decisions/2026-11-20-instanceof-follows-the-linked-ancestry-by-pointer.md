# The interpreter loop answers `instanceof` of a linked class name by following the object's linked ancestry by pointer

- Date: 2026-11-20
- Area: Classes
- Decided by: Design

## Decision

`$value instanceof Name` is answered in the loop when the file has linked `Name` to a class (the VM links it on the first object it tests), or for `self`, `static` and `parent`: the object's class, its parent and the interfaces of each, compared by pointer, with a fixed stack of 32 entries and at most 256 visits; beyond that, and in the guarded loop (a destructor due: a `catch` clause is an `instanceof` and the due destructor must run first), the VM answers.

## Why

The VM hashed the object's class name to find its class, and the tested name (lowercased, with an allocation when it had capitals) in the ancestor set: about 1,600 machine instructions per test against PHP's 410, 1,100 to 1,700 tests per framework request. In the loop: about 900 fewer (`micro/io.php`, three levels of interfaces).

## Turned down

A bitset of ancestor class numbers per class: the right structure, but it is built when classes are linked, which #128 is changing; it can replace the walk after #128.
