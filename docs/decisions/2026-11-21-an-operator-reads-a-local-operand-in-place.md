# An operator reads a local operand in place instead of copying it first (M7)

Step 2 part B, consumer family M7 (`Unary`, `Cast`, `Binary`, `BinaryInt` operands). The
`local_operands` pass (renamed from `local_path_keys`, which did M1) now treats an operator's
operand registers like a path key: when the operand temporary's last assignment before the
consumer is `Move temp, local` and nothing between can write the local, the operator reads the
local directly and the copy (and its `Clear`) disappears.

Same safety rules as M1: the local must not be exposed (may-hold-a-reference set, dynamic scope),
no branch target / handler entry / diverging instruction between copy and consumer, nothing that
can run PHP code or raise strictly between them (so the undefined-local warning cannot move past
a call's side effects), every reader of the temporary must be a rewritten consumer, and only the
copies themselves are removed. `BinaryIntBranch` and the other branch conditions are left alone:
conditions that branch directly belong to the front end's branch-directly work.

Warning parity differs per operator: the loop's `value_step` falls back on a `None` slot and the
general `Binary` handler warns, so `Binary`/`BinaryInt` needed no change. The general `Cast` and
`Unary` handlers read an undefined operand silently, so `warn_undefined_operands` (places.rs,
called from the `Unary`/`Cast` arms of `execute()` in instructions.rs) raises the `Move`'s warning
there, once, only for named locals. Verified against PHP 8.5 and main with an error-handler log:
`(int)$u`, `(bool)$u`, `-$u`, `!$u`, `$u + 1`, `1 + $u`, `$u . 'x'`, `(string)$u`, evaluation
order (`$x + ($x = 5)`, `-$x + f()`), and a reference-bound local all print identically.

Fixture: `tests/fixtures/operators/local-operands-in-place.phpt` (expected output from real PHP).
