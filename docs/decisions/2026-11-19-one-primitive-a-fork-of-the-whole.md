# One primitive: a fork of the whole VM

- Date: 2026-11-19
- Area: Speed and method
- Decided by: Design

## Decision

`Phera\snapshot()` is a VM intrinsic. In a boot run it copies
the VM as it is at the call (`Vm::fork_vm`), keeps the copy as the snapshot,
and stops the run with an unwind that no `catch` or `finally` sees. Every
request copies the snapshot the same way and resumes it: the call's result
register gets `true`, and the interpreter continues from the frames as they
were. (`crates/vm/src/snapshot/`)

## Why

Frames live on the heap and the loop publishes its position before
a native call, so the VM struct is the whole state: no suspension machinery
is needed, and the same copy serves the snapshot and the forks. Copying at
the call also tests at the call that the state can be kept, so a refusal is
an exception there, with a path to the object.

## Turned down

Suspending the boot run like a fiber and copying later (the
Rust future would hold the VM borrowed); running the boot again per request
from a cached prefix (does not save the boot).
