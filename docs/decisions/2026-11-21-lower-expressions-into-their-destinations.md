# Lower expressions into their destinations

- Date: 2026-11-21
- Area: Compiler
- Decided by: Design

## Decision

Use an expression's destination when the operation can write the completed
value there with the same assignment and lifetime semantics. The current
lowering generally returns a temporary register, so a consumer can emit a
copy or assignment followed by a release of that temporary. An expression
with an explicit destination can use the existing output register of its
instruction.

Read a constant from its unit slot when an instruction has an operand that
can address that slot. Keep the assignment commit point, expression order,
and diagnostics. A destination must still write through a reference if PHP
would and release its prior value at the same point. In particular,
lowering into a local must account for any callback or exception before
the new value is complete.

Measure executed opcodes by kind on Symfony's database yardstick and
Laravel's posts route before choosing which producers and consumers to
change. Measure native instructions separately on all three gated routes
and verify each change against PHP.
