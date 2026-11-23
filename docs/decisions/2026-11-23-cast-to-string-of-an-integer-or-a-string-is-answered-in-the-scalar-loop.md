# `(string) $value` of an integer or a string is answered in the scalar loop

- Date: 2026-11-23
- Area: Interpreter
- Decided by: Design

## Decision

`Cast` to `string` joins `Cast` to `bool` in `scalar_step` (`crates/vm/src/fast.rs`). It answers the two operands that need nothing but a copy or the digits:
- an integer, as `(string) $int` writes it (`phera_value::to_php_string`, no diagnostics possible);
- a string, which is its own conversion (the same reference-counted bytes).

Every other operand (a float, which follows the precision setting; a boolean, `null`, an array that warns, an object that may run `__toString`, a PHP reference, an unset variable) returns `Fallback` and the general handler converts it as before.

**What changed in `fast.rs`**, two hunks:
- `Instruction::Cast { to: CastKind::String, dst, src }` is one arm of `scalar_step`, at the line of the `Cast` to `bool` arm, calling `cast_string_step::<GUARDED, _>(slots, dst, src)` (4 lines).
- `cast_string_step` is a function of 24 lines with `#[inline(never)]`, next to `builtin_step` and in the same shape. The arm holds nothing that depends on the operand.

## Why

An integer in a string is everywhere in framework code: identifiers, cache keys, array keys of the identity map. The instruction left the scalar loop, and the exit was most of its cost (300 to 450 instructions in the profiles, `run`, `run_batch_loop`, `execute`, `Vm::get`, the diagnostics buffer).

Instructions per operation above an empty loop (`m4.php`, Linux, `perf stat -e instructions:u`, two iteration counts). Before: main `0c63b85f`. PHP 8.5.13 with OPcache on and JIT off (`php -n -d opcache.enable_cli=1 -d opcache.jit=disable`):

| operation | PHP 8.5 | before | after |
| --- | ---: | ---: | ---: |
| `(string) $int` | 294 | 946 | 499 |
| `(string) $string` | 71 | 760 | 238 |

What is left of 499 is `phera_value::to_php_string` for the integer (a formatter and two allocations; `integer_string` of the small-natives pull request writes the digits without the formatter) and the string's allocation.

## Turned down

- **A float and a boolean in the same arm.** A float's text depends on `precision` and `serialize_precision` and can warn for `NAN`; both go through the handler that already does it.
- **Putting the arm's body in the loop.** It would be four lines shorter and measured against the loop's layout, which a change of this size has moved before.
