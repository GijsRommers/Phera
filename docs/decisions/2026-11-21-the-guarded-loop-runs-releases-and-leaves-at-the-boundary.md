# The guarded loop runs releasing instructions itself and leaves at the next boundary when a destructor waits

- Date: 2026-11-21
- Area: Speed and method
- Decided by: Design
- Supersedes the guarded half of 2026-11-19 "A plain and a guarded copy of the interpreter loop"

## Decision

There are still two copies of the loop. The plain copy, which runs while no live object has a destructor, is unchanged.

The guarded copy now runs the shapes it used to send to the VM:
- `instanceof`;
- `foreach` start, end and next;
- element and property writes;
- compound assignments and increments;
- stores over arrays, objects and strings.

It tests the destructor queue only where a value can have been freed:
- a simple store that drops the last handle on an array, object, resource or PHP reference answers `ScalarStep::Released`, and only then does the loop read the one flag `ObjectIds::has_doomed()`;
- after any other step that completed, the loop reads the flag once;
- after a return or a call switch, as before.

When a destructor waits, the loop publishes the next instruction and leaves with `LoopExit::Destructors`. The VM runs the destructor at that boundary, which is the moment it ran it before, when it executed these instructions itself. `instanceof` still goes to the VM when a destructor already waits (a `catch` reached during unwinding). Making an object whose class has a destructor still goes to the VM, which records it.

## Why

A Symfony and Doctrine request keeps objects with destructors alive for its whole life, so it ran entirely on the guarded loop. On the 49-statement yardstick that was 39,248 exits from the loop per request, at 1,000 to 1,300 machine instructions each. With this change it is 28,733, and the request goes from 294.2M to 272.5M instructions. The Laravel routes never run guarded and do not move.

## Turned down

One loop for both cases, with the same tests: it cost 2.0 to 3.6 percent on fib, function-array, hierarchy and object-sort (measured on 21 November 2026 against the two-copy base), more than the 0.3 percent that would have paid for one copy less. With two copies, the plain copy is within 0.1 percent of before on all benchmarks except array (+0.6), function-array (+0.4) and object-sort (+0.2).
