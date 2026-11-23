# `void` returns, `spl_object_id` and class-typed arguments and returns stay in the loop

- Date: 2026-11-21
- Area: Calls
- Decided by: Design

## Decision

- A return passes its declared type's bit test when the type is `void` and the value is null, on every return path of the loop. A `void` function's `return;` is a `Return` of null, and no longer goes to the VM.
- `spl_object_id($object)` is answered where the batch loop runs native calls: an object's id, nothing else, and a lazy object stays uninitialized. A send of an object to it stays in the loop. Any other argument goes to the general intrinsic, which raises PHP's errors.
- A class-typed parameter or return whose value the unit's one-entry class cache does not remember is checked in the loop:
  - the declared class is found by its name key;
  - the object's linked ancestry is walked by pointer, as `instanceof` does;
  - a match is remembered in the same cache, as the VM's check does.

  Types with `self`, `static` or `parent`, intersections and classes the request has not declared go to the VM. For parameters the check is in the out-of-line second look (`accepts_on_second_look`), so that the hot send does not grow.

## Why

On the Symfony and Doctrine yardstick, 3,136 of 22,667 loop exits per request were returns: mostly `void` accessors, and `TypeRegistry::get`, whose `Type` return sees a different subclass each call, so the one-entry cache missed every time.

With this change it is 19,103 exits, and 226.9M against 217.0M instructions per request with warmed metadata.

## Turned down

Checking class types in the loop only from the cache: it was right for one class per declaration and wrong for every factory and registry.
