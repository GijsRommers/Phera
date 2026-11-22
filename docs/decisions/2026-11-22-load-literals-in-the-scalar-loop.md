# Load literals in the scalar loop

Date 2026-11-22.

The scalar loop can load a file literal into the current frame's register window. It borrows the existing materialized literal slice alongside the function's instructions and takes the new frame's slices after a call or return. The literal uses the guarded store when overwriting a register can queue a destructor.

A literal used by several readers still needs a register. Loading it through the second dispatch paid for leaving the scalar loop and looking up the same file on every execution. The literal slice already exists and every file keeps its own constants, including when a request forks a worker's snapshot.

| Structure | Was | Is |
|---|---|---|
| Running frame's code | Borrowed instructions and a register-window flag | Borrowed instructions and literals, with the same register-window flag |
| Windowed LoadConst | Second dispatch looked up the file and wrote the register | The scalar loop bounds-checks the literal and writes through the guarded scalar store |
| File literals | Immutable slice of materialized values in the unit cache | The loop borrows the same slice directly |
| Wide register windows | Checked registers through the second dispatch | Checked registers through the second dispatch |

Keeping a literal slice in each function's facts would duplicate ownership across functions from the same file. Extending each reader to take literals would change bytecode even for a literal that still needs to be loaded for other readers. Both options were set aside for this change.

The experiment reduced the cost of the large-integer literal probe by 53 user instructions per iteration. Its immediate integer control cost 37 more instructions even though it never took the new arm. Both request routes and fifteen of twenty script benchmarks exceeded the performance limit. The experiment is parked for those reasons.
