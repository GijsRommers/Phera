# A call of a synchronous native function is prepared as the function's number, and the registry keeps its functions in a flat list

- Date: 2026-11-20
- Area: Calls
- Decided by: Design

## Decision

The registry keeps its native functions in a `Vec<FunctionEntry>` in registration order; a function's `NativeId` is its index, and the name table maps a lowercase name to that number (used only when a name is linked). A call of a synchronous native function linked from a name is prepared in the interpreter loop as `PreparedCallee::Native(id)`, with no description; its sends check the entry's parameters, and its `Call` runs the handler from the batch loop, taking up to four arguments from an array on the stack. When any other path (a named argument, a by-reference parameter, a spread, an argument that needs converting) is about to read the call, the batch loop's exit gives the record the full description first, only when the top call is such a record.

## Why

A native call cost about 1,250 machine instructions against PHP's 127 (`str_starts_with` in a loop): half of it was the record (a cloned `Callee` with its name and parameter list, pushed and popped) and the hash lookup behind it. With the number: 962. The description is made only on the uncommon paths that read it.

## Turned down

Running native handlers inside the interpreter loop: the handlers take the whole VM, and refetching the loop's borrowed state on every entry cost 1 to 2 percent on benchmarks without native calls. Describing the record inside `Vm::execute` for every instruction that left the loop: it cost 1 to 2 percent on closures, generators and typed-objects.
