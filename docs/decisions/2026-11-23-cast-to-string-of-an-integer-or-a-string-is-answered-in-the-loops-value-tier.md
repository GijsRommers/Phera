# `(string) $value` of an integer or a string is answered in the loop's value tier

- Date: 2026-11-23
- Area: Interpreter
- Decided by: Design

## Decision

`Cast` to `string` is answered inside the interpreter loop, in `value_step` of `crates/vm/src/fast_values.rs`, the function the loop keeps out of line for "simple instructions on values other than integers". It answers the two operands that need nothing but a copy or the digits:
- an integer, as `(string) $int` writes it (`phera_value::to_php_string`, no diagnostics possible);
- a string, which is its own conversion (the same reference-counted bytes).

Every other operand (a float, which follows the precision setting; a boolean, `null`, an array that warns, an object that may run `__toString`, a PHP reference, an unset variable) returns `Fallback` and the general handler converts it as before. The other casts (`int`, `float`, `bool`, `object`, `array`) are not routed to the tier and cost what they did.

**Lines touched**, six:
- `crates/vm/src/calls/execution_loop.rs`, in the list of instructions `step` hands to `further_step` (line 591): one pattern, `Instruction::Cast { to: CastKind::String, .. }`.
- `crates/vm/src/calls/further_steps.rs`, in the list of instructions `further_step` hands to `value_step` (line 57): the same pattern.
- `crates/vm/src/fast_values.rs`: an arm of `value_step` (8 lines), `cast_to_string` (13 lines) and one import.
- `crates/vm/src/fast.rs` is not touched.

## Why

An integer in a string is everywhere in framework code: identifiers, cache keys, array keys of the identity map. The instruction left the loop and the exit was most of its cost (300 to 450 instructions in the profiles: `run`, `run_batch_loop`, `execute`, `Vm::get`, the diagnostics buffer).

Instructions per operation above an empty loop (`m4.php` and `m.php`, Linux, `perf stat -e instructions:u`, two iteration counts). Before: main `0c63b85f`. PHP 8.5.13 with OPcache on and JIT off (`php -n -d opcache.enable_cli=1 -d opcache.jit=disable`):

| operation | PHP 8.5 | before | after |
| --- | ---: | ---: | ---: |
| `(string) $int` | 294 | 946 | 642 |
| `(string) $string` | 56 | 760 | 385 |

Benchmarks (instructions, lowest and median of five, against main `0c63b85f`): `bench-framework-patterns` -1.0%, `bench-natives` -2.1%; nothing above +0.1% except `bench-objects` (+0.76% median, -0.77% lowest) and `bench-typed-objects` (+1.2% median, -0.4% lowest), which differ by that much between runs of the same binary. Routes (Hades, 8 pairs): `/api/hello` -0.04%, `/api/posts` -0.07%.

## Where it went first, and why it moved

The first version put the arm in `scalar_step` of `fast.rs`, next to `Cast` to `bool`. It cost 499 instructions for the integer, 140 fewer than here, and made `bench-function-array` +0.38%, `bench-object-sort` +0.47% and `bench-methods` +0.49%, none of which casts to a string. The same arm moved to an out-of-line function for all casts gave the same. Those benchmarks are the loop's own code, which a new arm in `scalar_step` changes. The value tier is entered only by instructions `scalar_step` has refused, so nothing that does not cast pays for it.

## Turned down

- **A float and a boolean in the same arm.** A float's text depends on `precision` and `serialize_precision` and can warn for `NAN`; both go through the handler that already does it.
- **The arm in `scalar_step`.** Above.
