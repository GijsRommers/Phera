# A condition branches directly: one compare-and-jump instruction, no temporary boolean

- Date: 2026-11-21
- Area: Compiler, bytecode, interpreter
- Decided by: Design

## Decision

The compiler front end compiles a condition as the jumps that decide it
(`compile_condition` in `crates/compiler/src/expressions/conditions.rs`), not
as a value that a later jump reads. Where the result of `===`, `!==`, `==`,
`!=`, `<`, `<=`, `>`, `>=` is only the condition of an `if`/`elseif`, `while`,
`for`, `do-while`, a ternary, `&&`, `||` or `!`, one instruction compares and
branches:

| PHP | structure: was | structure: is |
|---|---|---|
| `if ($a === $b)` | `Binary Identical` into a temporary, `JumpIfNot` | `BinaryBranch { Identical, $a, $b, else }` |
| `if ($a < 10)` / `if (10 > $a)` | `LoadInt`, `Binary Less`, `JumpIfNot` | `BinaryIntBranch { Less, $a, 10, else }` (literal on the left swaps operands and operator) |
| `if ($a !== null)` / `if (null === $a)` | `LoadConst null`, `Binary NotIdentical`, `JumpIfNot` | `JumpIfNull { $a, else }` / `JumpIfNotNull { $a, else }` |
| `if ($a)` / `if (!$a)` | `Move`, (`Unary Not`,) `JumpIfNot` | `JumpIfNot { $a }` / `JumpIf { $a }` on the local's own register |
| `if ($a && $b)` | value-producing short circuit, `Move`, `Unary Bool`, `JumpIfNot` | one jump per operand, no boolean value |
| `$c ? $a : $b` | condition value, `Move`, `JumpIfNot` | the condition's jumps land on the `otherwise` arm directly |

- The true case is a separate opcode, not a field: `BinaryIntBranch` /
  `BinaryIntBranchIf` and `BinaryBranch` / `BinaryBranchIf`, all within the
  16-byte instruction budget. A `when: bool` field was tried first and
  measured: testing it in the hot arm cost ~1% on arithmetic loops and ~2.5%
  on a bare `while ($i < $n) $i++` (min-of-9 "instructions retired" against
  main), because every compare-and-jump paid an extra flag test. With two
  opcodes the false case compiles byte-for-byte like main's `BinaryIntBranch`
  arm, and those shapes measure within noise (±0.4%). `when` cannot be
  compiled away by inverting `op`: PHP's orderings are not exact negations of
  each other (`NAN < x` and `NAN >= x` are both false).
- The liveness proof in `passes/integer_operands.rs` (`dead_after`) learned
  the four new instructions read their operands and branch target; without
  that, `LoadInt`+`Binary` stopped fusing into `BinaryInt` whenever the
  consumer was a compare-and-branch, which cost ~7% on `bench-arithmetic`.
  `fuse_integer_branches` now also fuses a leftover `Binary`+`JumpIfNot` into
  `BinaryBranch` and either jump into the `If` forms, so compare-branches
  reach every shape the front end does not special-case.
- `=== null` / `!== null` reuse the existing `JumpIfNull` / `JumpIfNotNull`.
- A boolean, float or string literal operand loads once and compares through
  `BinaryBranch`; only the integer literal rides inline, because the
  16-byte budget does not allow a third inline literal.
- A comparison whose value is used (`$x = $a === $b`) still compiles to
  `Binary`, unchanged.

In the interpreter, the quick cases run in the loop (`crates/vm/src/fast.rs`):
two integers for any comparison, and `===`/`!==` where at least one side owns
no heap value (a number, boolean or null — then identity needs no PHP code).
`crates/vm/src/fast_values.rs` answers the rest that raises nothing, through
the same `quiet_binary` `Binary` uses. Everything else — strings, arrays,
objects, floats needing PHP's coercion, an undefined variable and its
warning, a reference — goes to the general handler in
`crates/vm/src/control_flow.rs`, which runs the *same* computation
`execute_binary` runs (into the frame's reserved scratch slot) and then takes
the branch, so there is one definition of the comparison. The async cases (an
unset operand, an object's comparison handler, a pending string comparison)
leave through small `BatchExit`s that finish the same computation and then
decide.

`JumpIf`/`JumpIfNot`/`JumpIfNull`/`JumpIfNotNull` now read their operand with
`read_local`, so a branch on a named local that was never assigned warns
"Undefined variable" on the branch's own line, exactly where the `Move` that
used to precede it warned. `??` and `?->` are unaffected: they only ever test
registers a quiet read filled.

## Why

`phera --dump` on main showed `if ($a === $b)` as two bytecodes where one
suffices, `if ($a !== null)` as three, `if (!$a)` as three, and a `for` round
as `Binary Less`, `JumpIfNot`, body, `IncDecLocal` into a temporary nobody
reads, `Jump`. Every comparison used as a condition paid a temporary boolean
plus a second dispatch, and every loop round paid the trip out of the fast
loop for the comparison (`~/.phera-review/cliffs/REPORT.md`, group G4: 700
to 900 machine instructions per comparison against PHP's 30 to 70).

Compiling the condition as jumps in the front end (a
`compile_condition(expression, target, when)`) was chosen over a peephole
pass over finished bytecode: it sees the AST (which operand is a literal,
which is a plain local) before temporaries are allocated, it never emits the
temporary boolean at all, and it keeps `crates/compiler/src/passes/` untouched
while another change works there.

## Turned down

- Inverting the operator to encode `when`: wrong for NAN and any pair PHP does
  not define as exact negations.
- A single literal-carrying branch covering null, bool and int through one
  inline enum: the operand would not fit in the 16-byte instruction, so null
  reuses `JumpIfNull`/`JumpIfNotNull`, int keeps `BinaryIntBranch`, and
  bool/float/string load once.
- A `when: bool` field on the branch instructions (instead of the `If`
  opcode pair): correct, but the runtime flag test in the hot arm measurably
  cost every compare-and-jump in a loop; see the design notes above.
- Fusing `switch` case comparisons the same way: out of this step's scope; the
  subject is computed once and each case is a loose `==` against it.
- Making the fast loop answer string `===` byte equality: `quiet_binary`
  already covers it without PHP code; a dedicated arm was measured
  unnecessary.
