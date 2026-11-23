# Prove temporary ownership across branches

Date: 2026-11-22

## Decision

Run temporary cleanup analysis after operand and argument rewrites and before method-site formation. Join the possible register values from all predecessors of a block. An empty temporary can skip its release. A temporary containing only scalars can skip its release if no later instruction can observe the retained value.

For example, a temporary created for `(int) $value;` can keep its scalar until the slot is reused. A temporary holding an array still needs an early release because its ownership affects copy on write. A temporary object can also run a destructor when it is released.

A protected instruction can unwind before or after changing a register. Join both possibilities when determining which registers a handler must clear. Apply the same ownership and readability checks to the handler cleanup list. A null test refines the null edge when the register contains a value by copy. An exhausted iterator keeps its previous output slots. Relocate jumps, exception ranges and handler targets when compacting code and line entries.

| Structure | Was | Is |
| --- | --- | --- |
| Temporary facts | Reset at branch targets in the existing operand optimizer | A separate ownership analysis joins facts from normal and exceptional predecessors |
| Reads after a release | Local reasoning in the operand optimizer | Block-level liveness looks past all candidate releases until a definite overwrite |
| Protected code | Skipped by the operand optimizer | Cleanup analysis covers protected blocks and joins unwind inputs |
| Handler cleanup | Every allocated temporary in the try range | Entries with possible ownership or a surviving reader |
| Bytecode compaction | Relocated ordinary branch operands | Also relocates protected ranges and catch and finally targets |

## Turned down

- Using the value seen in a benchmark to decide what bytecode to emit. Every reachable path must satisfy the proof.
- Assuming unknown arithmetic results are numeric. Native numeric objects can return objects, and ordinary string and array operators can return owned values.
- Removing an owned value's final release or changing when the release happens.
