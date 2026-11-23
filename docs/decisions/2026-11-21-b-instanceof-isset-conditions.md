# Step 2: `instanceof`, `isset`/`empty`/`??` as conditions branch directly

Building on step 1 (`2026-11-21-a-condition-branches-directly.md`), the other
condition-only constructs compile as branches in the front end
(`compile_condition` in `crates/compiler/src/expressions/conditions.rs`), not
as a value that a later jump reads.

| PHP | structure: was | structure: is |
|---|---|---|
| `if ($o instanceof C)` | `Move`, `InstanceOf` into a temporary, `JumpIfNot`, `Clear`s | `InstanceOfBranch { $o, C, else }` — one instruction |
| `if (!($o instanceof C))` / `while ($o instanceof C)` | same plus jump polarity | `InstanceOfBranchIf` / `InstanceOfBranch` — the true case is its own opcode, as in step 1 (a measured `when` field cost the hot arm ~1-2.5%) |
| `if ($o instanceof $c)` (evaluated class) | `InstanceOf`-dynamic into a temporary, jump | `DynamicInstanceOfBranch(If)` |
| `if (isset($a['k']))` | key load, `FetchIndex Isset`, `JumpIfNull`, `LoadConst true`, `Move`, `JumpIfNot`, `Clear`s (7) | key load, `FetchIndex Isset`, `JumpIfNull { else }` (3) — the isset read branches directly, no boolean is materialized |
| `if (!isset($x))` | same, inverted | `JumpIfNotNull` on the isset read |
| `isset($a, $b)` in a condition | per-value bools | `when == false`: one `JumpIfNull` per value (any null jumps); `when == true`: nulls skip to the end, one `Jump` to the target |
| `if (empty($a['k']))` | fetch `Empty`, `Unary Not`, `JumpIfNot` | fetch `Empty`, then `JumpIf` on the raw read — `empty` is the inverted truthiness, so the `Unary Not` goes |
| `if ($a['k'] ?? $b)` | quiet fetch, `JumpIfNull`, `LoadConst null`, `Move`, `JumpIfNot` (5+) | quiet fetch, `JumpIfNull` to the right side, `JumpIfNot` on the left (when set), then the right side compiled as a condition |

`isset` keeps its quiet semantics: the reads use `FetchMode::Isset`, so an
undefined variable does not warn, and `??` keeps `FetchMode::Quiet` on its
left — a plain unset local still reads as null without a warning, which is
why the branch reads the fetch's temporary rather than the local (a branch
on an unset local register warns, per step 1).

New bytecode instructions: `InstanceOfBranch`/`InstanceOfBranchIf` (resolved
class name inline as a `SymbolId`, like `InstanceOf`) and
`DynamicInstanceOfBranch(If)` (evaluated class register), all within the
16-byte budget. In the VM the fast loop answers them in
`crates/vm/src/calls/fast_instance_of.rs` (`instance_of_answer`, shared with
the existing `instance_of` fast path, so there is one definition of the
answer; `instance_of_branch` jumps with backward-work counting); the general
handler in `crates/vm/src/classes/instantiation.rs` / `static_calls.rs`
computes the same answer and jumps. `isset`/`empty`/`??` reuse the existing
`JumpIfNull`/`JumpIfNotNull`/`JumpIf`/`JumpIfNot` on the quiet read, so no new
opcodes were needed for them.

Files of the interpreter loop touched: `crates/vm/src/calls/execution_loop.rs`
(routing list: `InstanceOfBranch`/`InstanceOfBranchIf` added next to
`InstanceOf`, lines ~470-472) and `crates/vm/src/calls/fast_instance_of.rs`
(only: the `instance_of` body split into `instance_of_answer` plus a new
`instance_of_branch`; `crates/vm/src/fast.rs` untouched this step).

## Why

`phera --dump` on the step-1 branch showed `if ($o instanceof Foo)` as five
bytecodes where one suffices, `if (isset($a['k']))` as seven, `if (empty(...))`
as four. Every one paid a temporary boolean plus a second dispatch.

## Measurements (release, min-of-3 "instructions retired")

- Full phpt suite: 8,262 passed, 0 failed, 25 skipped (95 areas).
- 2 new fixtures in `tests/fixtures/conditions/` (`instanceof_in_conditions`,
  `isset_empty_coalesce_in_conditions`), expected output from PHP 8.5,
  byte-identical; 4 new compiler unit tests pin the instruction shapes.
- Benchmarks vs step 1: worst -0.41% (bench-arithmetic), best +1.26%
  (bench-framework-patterns); vs main `c2f9b5c0` the branch total is unchanged
  from step 1 (worst -0.77%, wins up to +22.5% on bench-object-sort).

## Turned down

- A dedicated `isset`-and-branch opcode fusing the fetch and the jump: the
  fetch already produces the value in a register and `JumpIfNull` reads it
  directly, so a fused opcode would duplicate the fetch machinery for one
  saved dispatch.
- Fusing `empty`'s `Unary Not` into the branch by inverting the jump: done —
  `empty($x)` branches with `when` inverted, no new opcode.
