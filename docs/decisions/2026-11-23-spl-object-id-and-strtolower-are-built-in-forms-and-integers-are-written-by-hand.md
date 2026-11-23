# `spl_object_id` and `strtolower` are built-in forms of the scalar loop, and integers are written to strings by hand

- Date: 2026-11-23
- Area: Compiler and interpreter, strings
- Decided by: Design

## Decision

**Two unary built-ins get their own instruction form**, as `strlen`, `count` and the `is_*` functions have (`BuiltinUnary`):
- `spl_object_id($object)` for an object operand: its id.
- `strtolower($string)` for a string operand: the string with ASCII capitals lowered, or the same string when it has none (PHP also returns the string it was given when nothing changes).

Both are PHP functions that run with a frame, not operators, so for any other operand (an integer to convert, `null` with its deprecation, an array, an object with `__toString`, `strict_types`) the instruction calls the function with the frame a failure shows. A namespaced call is guarded as the other built-ins are: a function of the namespace of the same name wins. The loop's scalar tier needed no change: it already runs every `BuiltinUnary` through `builtin_value`.

**Integers are written to strings by two digits at a time, without a formatter** (`PhpString::from_integer`, `push_integer`). They are used by `(string) $int`, by `.` when one side is an integer, and by `implode`.

**`implode` builds its result at its size in one pass** when every value is a string, an integer, a boolean or `null` (it already knew the size and then checked and reserved before every append).

## Why

Instructions per operation above an empty loop (`m4.php`, Linux, `perf stat -e instructions:u`, two iteration counts). Before: main `0c63b85f`. PHP 8.5.13 with OPcache on and JIT off (`php -n -d opcache.enable_cli=1 -d opcache.jit=disable`):

| operation | PHP 8.5 | before | after |
| --- | ---: | ---: | ---: |
| `spl_object_id($e)` | 123 | 1,220 | 150 |
| `strtolower('MixedCase')` | 365 | 799 | 416 |
| `implode(' ', [1, 2, 3])` | 531 | 2,475 | 1,741 |
| `'a' . $int` | 494 | 838 | 838 |
| `(string) $int` | 293 | 945 | 945 |
| `$e instanceof Proxy`, false, as a value | 66 | 583 | 583 |
| `if ($e instanceof Proxy)` | 47 | 368 | 368 |
| `ltrim($s, '\\')` | 307 | 1,115 | 1,114 |
| `array_key_exists('name', $a)` | 92 | 847 | 847 |
| `count($a)` | 50 | 144 | 150 |

The integer writer does not move `(string) $int` or `.` here: their cost is the exit from the loop (below), and `.` takes another path in the VM than the one changed. It does move `implode`.

On the request gate (Linux, 8 pairs, main `0c63b85f`) `/api/hello` is -0.07% and `/api/posts` -0.04%: Laravel's requests call these functions too rarely to show.

## What is left, and why

One general native call costs about 1,000 instructions, and the handler is a small part of it. For `strtolower` before this change: 100 in the handler, 90 in `invoke_sync_function`, 170 in `call_native_direct`, and about 300 to 450 in leaving the loop for an instruction the scalar tier does not take and entering it again (`run` beyond its own dispatch, `run_batch_loop`, `execute`), plus allocation. The same is true of `ltrim`, `implode`, `array_key_exists` (an instruction of its own that the loop does not take: about 270 in the instruction, the rest in the exit) and `(string) $int` (`Cast` to `string`: about 120 to convert, 120 around it, the rest the exit).

The cheap path for each of them is the same one `strlen` has: the loop's scalar tier answers the instruction itself. That is a change in `fast.rs` and the instruction's own arm, which this change does not make:
- `Cast` to `string` for an integer: saves 300 to 400.
- `ArrayKeyExists`, keys of integer or string, an array operand: saves 400 to 500.
- A binary built-in form, for `ltrim($s, $chars)`, `str_contains`, `str_starts_with` and the like (two operands, a string or integer result): saves 300 to 450 each.

These are estimates from the profiles, not measurements.

`instanceof` is already in the loop (`fast_instance_of.rs`). Its 369 instructions above the empty loop are the answer itself: 115 in `instance_of_answer` (the walk of the object's class and interfaces), 55 in `instance_target` (which compares the written name with `self`, `static` and `parent` before it looks the class up), 30 in `linked_class` and 25 in the class table's `by_key`, the rest the loop's own dispatch. It is not changed here.

## Structure: was, is

| structure | was | is |
| --- | --- | --- |
| `spl_object_id($x)` | `InitCall`, send, `Call`, the intrinsic `object_id_call` | `BuiltinUnary(ObjectId)` |
| `strtolower($x)` | `CallNativeDirect` | `BuiltinUnary(StrToLower)` |
| integer to decimal | `i64::to_string` (a `String`, then a `PhpString`), `write!` | digits two at a time into a 20-byte array, then one buffer of the right length |
| `implode` of scalars | size, then `reserve` and write per element | size, then one buffer, then pushes |

## Turned down

- **`ltrim` as a unary form.** `ltrim($s)` with one argument is not what frameworks write; they pass the characters.
- **A cache of the lowered string per class name.** Doctrine lowers the same few names, but a cache keyed by string costs a hash (about 100) and a lookup, as much as the lowering it saves.
- **Making `spl_object_id` an operator without a frame.** PHP shows its frame in the trace of a `TypeError`; `strlen` is an operator in PHP and has none.
