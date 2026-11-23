# A by-value foreach writes its entries straight into the loop's variables

- Date: 2026-11-22
- Area: Compiler and interpreter
- Decided by: Design

## Decision

`foreach ($items as $key => $value)` with plain local variables names those variables' own registers as the targets of `IterNext` / `IterNextKeyed`. Before, the step wrote into temporaries and an `Assign` per variable copied them over. A temporary stays where a direct write would change what PHP does:
- **by-reference loops** (`as &$value`), and loops over a reference subject;
- **`list()` / `[...]` destructuring, properties and elements as the value.** With such a value the key keeps its temporary too: PHP writes the value first, and the value's own targets must see the key's old value (`foreach ($a as $k => [$seen[$k], $b])`);
- **the same variable as key and value.**

Where a step writes a variable directly, it does so as PHP does:
- the value first, then the key, so an old value the variable held is released (and its destructor run) first;
- a variable that holds a PHP reference (`global $v`, `static $v`, `$v = &$x`, a closure's `use (&$v)`) is written through. The step in the interpreter loop sees the reference before the iterator advances and leaves to the VM, which assigns as `Assign` does.

`if (condition) continue;`, as the whole body of an `if` with no `else`, compiles to the condition's own branch to the loop's continue edge. It keeps its own code when a `finally` stands between the `if` and the loop, or when the innermost context is a `switch`. A `continue` deeper than 1 is not changed.

## Why

Per loop iteration, instructions above an empty loop, against #186's head:

| loop | before | after | PHP 8.5 |
| --- | ---: | ---: | ---: |
| `foreach ($a as $v)` | 129 | 102 | -102 |
| `foreach ($a as $i => $v)` | 218 | 141 | 25 |
| with `if ($v & 1) continue;` | 222 | 170 | 23 |
| string keys | 665 | 532 | 307 |
| `foreach ($pairs as [$x, $y])` (unchanged bytecode) | 1,044–1,050 | 1,060 | 260 |

(PHP's -102 means its `foreach` costs less than the empty loop's `$k & 1`.)

## Structure: was, is

| structure | was | is |
| --- | --- | --- |
| by-value `foreach` with plain local targets | `IterNext` into a temporary, then `Assign` per variable | `IterNext` into the variable's register |
| the loop's iteration step | stores into the destination register | the same, after one test that no destination holds a PHP reference; value before key |
| `if (...) continue;` | a branch over a jump to the continue edge | the condition branches to the continue edge |

## Turned down

- **A count of the function's locals in each step**, to tell a variable from a temporary. The earlier draft did this: it cost about 40 instructions per iteration of every loop, `list()` loops included. A reference test on the destination is all a direct write needs, since a temporary that still holds a reference is overwritten correctly by the VM path.
- **Writing the key directly when the value is not a plain local.** That reverses PHP's order.
