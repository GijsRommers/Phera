# A native call whose synchronous part missed a class loads it and runs again, instead of the instruction running again

- Date: 2026-11-20
- Area: Calls
- Decided by: Design

## Decision

When a step that cannot run the autoloader (evaluating a property default or constant inside `ReflectionClass::newInstanceWithoutConstructor()`, `unserialize()`, `constant()` and similar) unwinds with a class miss from inside a native call, the native call boundary handles it:
- An async boundary (native functions and methods run by the async shell) loads the class and calls the native again with the same arguments, at most 64 times.
- A synchronous boundary in the batch loop (sync native functions, sync native methods, `CallNativeDirect`) puts the prepared call, its details, `$this` or the closure and the arguments back as they were. The interpreter's existing retry of the instruction then finds them.

## Why

The interpreter answers a class miss by loading the class and running the instruction again. That is right for an instruction like `new` or a class constant fetch, but a `Call` of a native has taken its prepared call and arguments by then. The second run took the next prepared call on the stack, the caller's. The result was "call not initialized", or an outer function run with no arguments: Symfony's `Hydrator::hydrate()` with debug off, six of 22 requests of the test app answering 500.

## Turned down

Making the step itself able to wait: property defaults and constants are evaluated from synchronous code in many places. A flag on the miss telling the interpreter not to run the instruction again: the native's arguments are gone by then, so there would be nothing to run.
