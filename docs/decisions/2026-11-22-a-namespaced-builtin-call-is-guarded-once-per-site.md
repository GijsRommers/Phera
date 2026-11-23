# An unqualified built-in call in a namespace runs as the built-in, behind a guard its site decides once

- Date: 2026-11-22
- Area: Compiler and interpreter
- Decided by: Design

## Decision

Inside `namespace App;`, a call `is_null($x)` (and the other built-ins that have their own instruction: `is_*`, `strlen`, `count`, `sizeof`, `gettype`, `get_class`, `defined`) compiles to two forms behind a guard:

```text
    GuardBuiltin  App\is_null, call: general
    BuiltinUnary  is_null, $x -> result
    Jump          done
general:
    InitCall App\is_null; SendLocal $x; Call; Move -> result
done:
```

**How the guard decides.** The first time a site runs, the guard asks whether a function `App\is_null` is declared. It keeps the answer in the instruction's site cell. After that:
- with no such function, the guard falls through to the built-in's own instruction, which the interpreter loop runs;
- with one, it jumps to the general call.

**PHP's rule, which this follows.** A call site keeps the target it found the first time it ran. A function declared later is called only by sites that have not run yet. Real PHP does this, and Phera's general call already did.

**When the guard is placed.** It comes before the argument, as PHP finds the function before it evaluates the arguments.

**Arguments that get the guarded form.** The argument is compiled twice, once for each form. So only arguments that declare nothing get the guard: variables, literals, constants, and reads of properties, elements and static properties. Everything else keeps the general call alone:
- a closure, an arrow function or an anonymous class;
- named or spread arguments, or another argument count;
- `is_null(...)`;
- a call under `use function`, which already names one function.

**As a condition.** `is_null(...)` as the condition of a branch is the null jump itself (`JumpIfNull` / `JumpIfNotNull`). That holds in global code too, and inside a namespace behind the same guard.

## Why

Framework code is namespaced and writes these calls unqualified. One Laravel `/api/hello` request makes 285 `is_null` and 106 `is_array` calls this way, each through the general native call.

Instructions per call in a namespace:

| call | before | after | PHP 8.5 |
| --- | ---: | ---: | ---: |
| `is_null($x) ? 1 : 0` (the other `is_*` alike) | 1,327 | 521 | 851 |
| `strlen($s)` | 1,099 | 287 | 739 |
| `count($a)` | 1,156 | 299 | 784 |
| `count($box->items)` | 1,475 | 748 | 822 |
| `gettype($v)` | 2,455 | 1,651 | 1,512 |
| `get_class($o)` | 5,352 | 1,820 | 1,110 |
| `if (is_null($x))` | 1,128 | 325 | 806 |

PHP pays a function call here too, because the name might be a namespace's function.

The guard costs about 175 instructions over the qualified `\is_null($x)`: it runs in the loop's second tier, which reads the site cell.

## Structure: was, is

| structure | was | is |
| --- | --- | --- |
| unqualified built-in call in a namespace | `InitCall` / send / `Call` every time | `GuardBuiltin` + the built-in's instruction; the general call stays behind the guard |
| what a site found | `CachedSite::Function` for the general call's `InitCall` | also `CachedSite::BuiltinGuard(Builtin | Namespaced)` for the guard, in the same per-instruction cells |
| `is_null(...)` as a condition | the built-in, a boolean, then a branch | `JumpIfNull` / `JumpIfNotNull` on the value |

## Turned down

- **One instruction that evaluates the argument, then decides** (the earlier draft's `BuiltinGuarded`). PHP decides before the argument is evaluated, so an argument that declares the function would get the wrong function. A namespace function with a by-reference parameter would also receive a copy.
- **Deciding at compile time.** A function of the namespace can be declared by any file a request includes, and only at run time.
- **A name lookup at each call.** The site cell already exists for every instruction, and one read of it by position answers the question for the site's whole life.
- **A branch instruction for the other type tests** (`if (is_array($x))`). Those keep the built-in and a branch; a new instruction for them is a separate change.

## Not built

`disable_functions` is a storage-only setting in Phera: a disabled built-in stays callable, as it is on main. This change does not alter that.
