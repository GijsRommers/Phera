# array_map, array_filter, array_reduce and the user sorts run their callbacks on the interpreter's frame stack

- Date: 2026-11-21
- Area: Calls
- Decided by: Design

## Decision

`array_map` over one array, `array_filter` with a callback, `array_reduce`, `usort`, `uasort` and `uksort` run as native loops (`crates/vm/src/calls/native_loops.rs`) when their callback is PHP code that takes its parameters by value.
- The built-in's state is a plain value on a stack in the `CallStack`.
- It enters each callback as an ordinary PHP frame marked as returning to the built-in, and the interpreter goes on in the same loop.
- The callback's return hands the value to the state's next step.

The user sorts keep PHP's sort (moved to `phera_extension_api::zend_sort`, so that the VM can drive it) as one boxed future per sort. Its comparison leaves the pair in a cell and waits for the answer, so each poll runs the sort to its next comparison (`calls/native_sorts.rs`).

Every other shape takes the async path as before. Those shapes are:
- a string or array callable;
- several arrays;
- a generator;
- a by-reference parameter;
- named arguments;
- pending error handlers.

## Why

An async built-in awaited a prepared call, a callback window, a fresh run of the interpreter loop and a future per step for every element. Per element (machine instructions):
- `array_map`: 2,334 → 1,445 (PHP 377);
- `array_filter`: 3,533 → 1,554 (PHP 476);
- `usort` of the probe array: 22,144 → 11,933 per sort (PHP 3,066).

## Turned down

Rewriting the sort as an explicit state machine: the comparison order is observable and the existing sort already makes PHP's comparisons. One boxed future per sort costs a poll per comparison, not an allocation.
