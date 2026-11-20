# Native property handlers describe string assignments

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

A native setter can describe a virtual string property to the VM. The VM
does any weak Stringable coercion before calling the synchronous setter.
When a nested write traverses a native getter returning an object, it
continues on the returned handle.

## Why

The VM can execute `__toString()` without retaining a borrow of native
state. A write to an object returned by a getter needs no writeback to
the property that returned it.

## Turned down

Coercing every object passed to a native property setter or writing an
intermediate getter's result back into the receiver.
