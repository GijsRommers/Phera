# A function's instructions are shared `Code`, held by the function's facts

- Date: 2026-11-20
- Area: Speed and method
- Decided by: Design

## Decision

`phera_bytecode::Function::instructions` is a `Code`, a reference-counted
`Vec<Instruction>` that reads as a slice. The compiler builds it as before
(`push`, in-place rewrites while it has the only owner). When a request
loads a file, each function's `FunctionFacts` takes a handle on it, and the
loop finds the running function's instructions there. The facts also give
a frame's register count to the VM's general path.
(`crates/bytecode/src/code.rs`, `crates/vm/src/function_facts.rs`)

## Why

Every call, return and re-entry of the loop looked up the instructions
through the facts, the loaded file, the compiled unit and its function
table: five dependent loads, the hottest single instruction in the `fib`
and `methods` profiles. Now it is three, and loading a file copies nothing.

## Turned down

Copying the instructions into the request's code table (a copy of every
loaded file per request); `Arc<[Instruction]>` (two loads, but the compiler
appends while lowering and would need its own buffer and a conversion);
remembering slices in the loop (lost at every exit from the loop).
