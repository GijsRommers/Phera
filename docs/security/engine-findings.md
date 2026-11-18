# Engine security findings

Authorized defensive security testing of the Phera engine (PHP source and
data given to `phera` / `phera --dump`), prepared from commit `cfed5d7`.
The HTTP server and PDO are out of scope (separate tasks).

Severity scale (from the task): **1** crash of the runtime (panic, abort,
stack overflow, signal) — the design says no PHP input may do this; **2**
hang where real PHP 8.5 finishes quickly; **3** unbounded resource use from
a small input; **4** data leaking across requests; **5** wrong behaviour with
security meaning. Non-security parity gaps are in the appendix.

Oracle: `/opt/homebrew/bin/php -n` (PHP 8.5.12). Every finding was confirmed
three times on a clean release build of this worktree (`cargo build --release
-p phera-cli`, engine sources unmodified).

## Summary

| Name | Severity | One line |
| --- | --- | --- |
| `strict-identity-reference-cycle-crash` | 1 | `===` on an array containing a reference to itself recurses in Rust and aborts the process (stack overflow). |
| `long-chain-assign-clone-stack-overflow` | 1 | Assigning a ~600k-term operator chain makes the parser clone the AST recursively and abort (PHP 8.5 runs the same file in 0.1 s). |
| `selfref-array-cursor-panic` | 1 | `end()`/`reset()`/`array_pop()`/`array_shift()` on an array containing a reference to itself panic (`RefCell already mutably borrowed`, exit 101). |
| `class-chain-linking-quadratic` | 3 | A deep `class Cn extends Cn-1` chain costs O(n²) time and memory when linking (20 000 classes: 16 s and 4.4 GB in Phera vs 0.2 s in PHP). |
| `no-memory-limit-unbounded-ram` | 3 | The engine has no memory limit: a 3-line loop grows past 2.4 GB in 4 s where PHP dies at 128 MB in 0.06 s. |

*(Campaign results and any further findings are appended below as they are
confirmed.)*

## `strict-identity-reference-cycle-crash` — severity 1 (crash)

**Reproducer:** `tests/security/strict-identity-cycle-crash.php`

```php
<?php
$a = [];
$a["s"] = &$a;
var_dump($a === $a);
```

**Command and output:**

```
$ phera tests/security/strict-identity-cycle-crash.php
thread 'main' (5779239) has overflowed its stack
fatal runtime error: stack overflow, aborting
$ echo $?
134            # SIGABRT; reproduced 3/3 runs
```

Also reachable through `in_array($a, [$a], true)`, `array_search($a, [$a],
true)`, `[$a] === [$a]`, and `$a === $b` with two distinct cyclic arrays —
all abort the same way.

**Real PHP 8.5:** prints `bool(true)`, exit 0. (For loose `==` on two
*distinct* cyclic arrays PHP throws `Error: Nesting level too deep -
recursive dependency?`; Phera returns a value instead — see appendix.)

**Root cause:** `crates/value/src/ops/compare.rs` — `fn identical()` at line
377 recurses without a bound: the `Reference` arm (lines 389–393) calls
`identical(&x, &y)` again after dereferencing, and `identical_arrays()`
(line 410) compares each entry pair with a recursive `identical(va, vb)` call
at line 444. A reference cycle (`$a["s"] = &$a`) therefore recurses once per
cycle iteration, each level consuming Rust stack; the VM runs on the main
thread (8 MB), so the process aborts. The loose `compare` path was flattened
iteratively (`compare_arrays`, line 337) but the identity path was not.

**Suggested fix:** make `identical` iterative like `compare_arrays` — an
explicit stack of `(a, b)` value pairs — or keep a visited set of
`Rc::as_ptr` pairs and treat a revisited pair as identical, matching PHP.

## `long-chain-assign-clone-stack-overflow` — severity 1 (crash)

**Reproducer:** `tests/security/long-chain-assign-crash.php` (generated:
`$x = 1+1+...+1;` with 600 000 terms, 1.2 MB).

**Command and output:**

```
$ phera tests/security/long-chain-assign-crash.php     # and: phera --dump ...
thread 'phera-compiler' (5302572) has overflowed its stack
fatal runtime error: stack overflow, aborting
$ echo $?
134            # reproduced 3/3 in run mode and 3/3 in --dump mode
```

The same happens for `'a' . 'a' . ...` (600 000 terms, 3.6 MB), and for
`+ * && || & % / -` chains; threshold is between ~300 000 and ~600 000 terms
depending on operator. Chains *not* under an assignment (`echo 1+1+...;`,
call arguments, `return ...`) parse fine — the crash needs the clone in the
assignment path (also array items `$a = [<chain>];` and ternary branches).

**Real PHP 8.5:** the 600 000-term concat file compiles **and runs** in 0.1 s
(prints `600001`). The 600 000-term `+` file fails with a clean, catchable
`Fatal error: Maximum call stack size of 8339456 bytes ... reached during
compilation` (exit 255) — never a crash.

**Root cause:** `crates/parser/src/expression.rs` — `reassociate_assign()`
(line 1500) builds the assignment through the `attach` closure, which does
`value.clone()` on the whole right-hand side (lines 1515–1526). `Expression`
derives `Clone` (`crates/ast/src/expression.rs:5`), and derived `Clone`
recurses per node down the left spine of the chain. The parser deliberately
builds left-associative chains in a loop without a depth limit (the custom
`Drop` impl at `crates/ast/src/expression.rs:19` was made iterative for
exactly this reason, but `Clone` was not), so the clone overflows the
parser/compiler thread's 256 MB stack at a few hundred thousand terms and
aborts the process.

**Suggested fix:** give `Expression` an iterative `clone` for left-spine
chains (mirror the `Drop` impl: walk `Binary.left`/`Index.target`/... in a
loop and rebuild), or change `reassociate_assign` to take the `Rhs` by value
so no clone is needed.

## `selfref-array-cursor-panic` — severity 1 (crash / panic)

**Reproducer:** `tests/security/selfref-array-cursor-panic.php`

```php
<?php
$s = ["self" => &$s];
end($s);
echo "ok\n";
```

**Command and output:**

```
$ phera tests/security/selfref-array-cursor-panic.php
thread 'main' (7128932) panicked at crates/value/src/value.rs:53:57:
RefCell already mutably borrowed
$ echo $?
101            # reproduced 3/3 runs
```

The same panic fires for `reset($s)`, `array_pop($s)`, `array_shift($s)`
(and for `next($s)`/`prev($s)` when the cursor lands on the self-referencing
element, e.g. `$s = [1, "self" => &$s]; next($s);`).

**Real PHP 8.5:** prints `ok`, exit 0, for all of them.

**Root cause:** the by-reference array helpers hold the reference cell's
`try_borrow_mut()` guard for the whole operation
(`crates/extensions/standard/src/arrays/stack.rs` `mutate`/`mutate_with_errors`,
used by `move_pointer` in `crates/extensions/standard/src/arrays/pointer.rs:36`),
and then call `value.deref().clone()` on the resulting element
(`pointer.rs:26` and `:43`; `stack.rs:86` and `:95`). When that element is a
`Reference` to the very cell being mutably borrowed, `Value::deref()`
(`crates/value/src/value.rs:52-55`, `r.borrow()`) borrows the same `RefCell`
again and panics.

**Suggested fix:** dereference/clone the result only after the `borrow_mut`
guard is dropped, or use `try_borrow` inside `deref`-under-mutation and raise
a PHP `Error` on conflict (as `mutate` already does for the outer cell).

## `class-chain-linking-quadratic` — severity 3 (quadratic resource use)

**Reproducer:** `tests/security/class-chain-quadratic.php` (generated:
`class C0 {} class C1 extends C0 {} ... class C10000 extends C9999 {}`,
288 KB; the file header explains how to regenerate at any depth).

**Command and measurement** (`time phera tests/security/class-chain-quadratic.php`,
peak RSS sampled with `ps -o rss=`):

```
n=2000:   Phera 0.21 s                PHP 0.21 s
n=5000:   Phera 0.85 s                PHP 0.21 s
n=10000:  Phera 3.4 s, ~1.1 GB RSS    PHP 0.21 s, a few MB
n=20000:  Phera 16.1 s, 4.4 GB RSS    PHP 0.21 s
```

i.e. O(n²) time and memory in Phera against O(n) in PHP — a ~700 KB source
file (20 000 classes) needs 4.4 GB and 16 s in Phera.

**Root cause:** `crates/vm/src/classes/linking.rs` — linking a class clones
the parent's whole tables instead of referencing them: `class.methods.clone()`
(line 136), `class.defaults.clone()` (line 178), `class.constants.clone()`
(line 182). A chain of n classes therefore copies O(n) table entries n times.

**Suggested fix:** store a parent pointer in `Class` and look methods up the
chain (as PHP does), or share the parent's tables behind `Rc` and only copy
on first override.

## `no-memory-limit-unbounded-ram` — severity 3 (unbounded resource use)

**Reproducer:** `tests/security/no-memory-limit.php`

```php
<?php
$a = [];
while (true) {
    $a[] = str_repeat('x', 1024);
}
```

**Command and measurement:** `timeout 10 phera tests/security/no-memory-limit.php`
with `ps -o rss= -p $!` sampled every 2 s:

```
t=2s  rss=1428640 KiB
t=4s  rss=2430560 KiB     # > 2 GiB in 4 s, still growing; killed by the watchdog
```

The mutation campaign also caught the same class from a mutated seed
(`$newstring = $newstring . $string;` in a loop with a broken inner counter,
peak RSS 2.26 GiB in 1.5 s) and a `$GLOBALS["A$i"] = 1` loop with `$i < 1e309`
(Phera still running at the 5 s timeout; PHP fatal-exits in 0.1 s).

**Real PHP 8.5** (`php -n`, `memory_limit=128M`): the same 3-line script dies
in **0.06 s** with `Fatal error: Allowed memory size of 134217728 bytes
exhausted`; `str_repeat('ab', 200000000)` (400 MB) and
`array_fill(0, 50000000, 'xxxxxxxxxx')` (~1 GB) are likewise rejected, while
Phera happily allocates and keeps them.

**Root cause:** there is no memory accounting anywhere in the engine —
`grep -r memory_limit crates/ docs/` finds nothing. Only two narrow caps
exist: `MAX_STRING_BYTES = 2 GiB` for single string allocations
(`crates/extensions/standard/src/strings/mod.rs:28`) and a 16 MiB cap for
string-offset writes (`crates/vm/src/path_update.rs:172`). Arrays, objects,
frames and globals grow without bound, so a small input can exhaust the
machine's RAM (and in `phera serve` would take down every request on the
worker).

**Suggested fix:** add a per-VM memory budget (PHP's `memory_limit` model:
account string/array/object/frame allocations, raise a catchable
`Allowed memory size exhausted` fatal when exceeded).

## Campaign log

All campaigns ran with `nice -n 15`, 2 parallel workers, a 5 s per-input time
limit, an RSS watchdog killing targets above 2 GiB, and a 512 MiB output cap
(see `tools/security/README.md`). Seeds: 2 477 `--FILE--` sections extracted
from `tests/upstream/**/*.phpt` and `tests/fixtures/**/*.phpt`.

| Campaign | Wall time | Inputs executed | Findings saved | Outcome |
| --- | --- | --- | --- | --- |
| `mutate` (run mode, seed 4242) | 2 h 00 m (18:47–20:47) | 380 664 | 74 | 68 timeouts (all triaged: 67 are infinite loops that also hang PHP 8.5; 1 is the no-memory-limit `$GLOBALS` loop = F3), 5 memory (all F3 class), 1 output bomb (infinite print loop, also in PHP). No crash-class findings from random mutation. |
| `dump` (front end only, seed 777) | 30 m (20:46–21:16) | 101 007 | 0 | Lexer/parser/compiler clean against mutated corpus. (The chain crash F2 was found by targeted probing, not random mutation.) |
| `gen` (structure-aware, seed 99) | 40 m (21:16–21:56) | 75 476 | 480 | All in known classes: `array-fill-huge`/`range-huge` = F3, `class-chain-n20000/n100000` = F5 (memory + timeout), `unwind-deep` timeouts were load artifacts (manual re-run: 10 ms Phera vs 56 ms PHP at n=100 000). No crash-class findings. |
| `funcs` (hostile native args, seed 5150) | 40 m (21:56–22:36) | 123 621 | 22 | All 22 are the F4 panic family (`array_pop`/`array_shift` on the self-referential-array argument); wrong types, boundary ints, NaN/INF, huge repeat counts, catastrophic regexes and depth limits are all handled as PHP errors. |

Areas with nothing found: nesting depth of every construct (parser limit 1000
works), long chains without assignment, deep PHP recursion (frame limit 250
000 works), cyclic structures freed/dumped (iterative drop works), exceptions
from awkward places, `json_decode` depth and malformed input, `preg_*`
backtrack/recursion limits and invalid UTF-8 with `u`, `sprintf` widths and
positionals, string-offset writes (16 MiB cap), comparison semantics,
visibility/readonly/strict_types probes — all match PHP 8.5 or fail as clean
PHP errors.

Category 4 (leak across requests) is not testable through the CLI (one script
per process); it belongs to the HTTP-server task, which is out of scope here.

## Appendix: parity gaps without security meaning (not chased further)

- Source files must be valid UTF-8: `phera file.php` with non-UTF-8 bytes
  prints `Could not open ...: stream did not contain valid UTF-8`
  (`crates/runtime/src/cache.rs:37` uses `read_to_string`); PHP accepts
  arbitrary bytes in source.
- `eval()` is not supported (`Expression not supported yet`), generators are
  not supported (`Generators not supported yet`), `pack`/`unpack`,
  `serialize`/`unserialize`, `ob_start`, `ReflectionClass` and magic
  `__get`/`__set`/`__call` are missing or partial.
- `$a[] = &$a` (append by reference) is a fatal `Cannot use [] for reading`
  in Phera; PHP allows it.
- `match` with ~100k arms hits a clean `Phera register limit exceeded` fatal
  where PHP compiles it.
- Loose `==`/`<=>` on distinct cyclic arrays returns a value in Phera where
  PHP throws `Nesting level too deep - recursive dependency?`.
- Native string parameters (e.g. `str_repeat`) do not accept stringable
  objects where PHP converts via `__toString`.
- Parser nesting limit (1000) is stricter than PHP's for some constructs
  (e.g. `1 ** 1 ** ...` 1000 deep is rejected).
