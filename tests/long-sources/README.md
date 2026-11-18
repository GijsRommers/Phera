# Long and deep sources

Machine-generated PHP (compiled dependency-injection containers, compiled
templates, data tables, translation catalogues) holds expressions and lists
far larger than anything written by hand. These tools check that Phera
compiles and runs such sources as PHP does, and never overflows the Rust
stack on them.

- `generate.php` builds one of 73 shapes at a given size and prints the
  source. Every source prints a short result, so outputs can be compared.

      php -n tests/long-sources/generate.php --list
      php -n tests/long-sources/generate.php add 100000 > add.php

- `table.py` runs every shape at 1e3, 1e4, 1e5 and 1e6 through PHP and a
  Phera binary and prints a Markdown table: status, first output line, wall
  time and peak memory of each.

      tests/long-sources/table.py --phera target/release/phera
      tests/long-sources/table.py --phera target/debug/phera --sizes 1000,10000 --only add,concat

- `crates/cli/tests/long_sources.rs` runs 75 cases of the generator in the
  test suite, against real PHP; `tests/fixtures/long-sources/` holds `.phpt`
  files for behaviours with a fixed small output.

## Chains and lists: loops, not recursion

Chains grow down one side of the tree: `1 + 1 + ...`, `$a->b()->c()...`,
`$a ?: $b ?: ...`, `!!!$a`. `Expression` clones and drops them in a loop
(`crates/ast/src/chain.rs`); the parser builds them in loops; the compiler
compiles chains longer than 64 steps bottom-up in a loop
(`crates/compiler/src/expressions/chains.rs`) and hands each step's
temporary registers back (`crates/compiler/src/temporaries.rs`). Lists
(array items, arguments, statements, `match` arms, `case`s, `elseif`
branches) are vectors and are walked in loops everywhere.

## Nesting: two limits

Real nesting (parentheses in parentheses, arrays in arrays, blocks in blocks)
is followed by recursion in the parser, the compiler and the analyzer. It
has two limits, described in `crates/parser/src/nesting.rs`:

- PHP's parser stack of 10,000 entries. Phera counts it the same way and
  stops with PHP's own "memory exhausted" at the same depth, within a level:

  | shape | PHP accepts up to | Phera accepts up to |
  |---|---|---|
  | nested `if` | 1,665 | 1,666 |
  | nested `while` | 1,665 | 1,666 |
  | nested `foreach` | 1,249 | 1,249 |
  | nested `for` | 999 | 999 |
  | nested closures | 832 | 833 |
  | nested arrow functions | 999 | 999 |
  | nested `static fn` | 908 | 908 |

- Phera's own limit of 2,500 levels of recursion, with the error "nesting is
  too deep: Phera supports at most 2500 levels". It is below PHP's limit for
  the constructs PHP nests most cheaply: PHP accepts 9,994 nested
  parentheses, 9,993 nested arrays, 4,996 nested calls, 4,997 chained
  assignments; Phera accepts 2,497 of each (1,249 for `$a + (...)` and
  `$a ? 0 : (...)`, which recurse twice per level).

The recursion runs on the compiler's (and analyzer's) dedicated thread of
256 MB. At Phera's limit the most expensive shapes need, in a debug build:

| shape at the limit | stack used |
|---|---|
| 2,490 nested arrays | 110 MB |
| 2,490 nested destructuring lists | 110 MB |
| 2,490 nested calls | 95 MB |
| 2,490 nested `new` | 94 MB |
| 2,490 nested parentheses | 80 MB |
| 1,240 `$a ? 0 : (...)` | 50 MB |
| 1,660 nested `if` | 45 MB |
| 830 nested closures | 42 MB |

A debug build uses 25 to 54 KB of stack per level of nesting, so even PHP's
own depths cannot be reached on an ordinary thread's 8 MB (or a test
thread's 2 MB). That is why the dedicated thread stays: it is sized for
bounded nesting, not for chains, which no longer use stack in proportion to
their length.

## Known limits that remain

- One function has at most 65,534 registers, so at most that many distinct
  variables: `global $g0, ..., $g99999;` or a 100,000-variable `list()` stop
  with "Phera register limit exceeded" where PHP runs them.
- Attributes are not supported by the compiler yet, at any length.
- Memory: a 1,000,000-term chain peaks at 594 MB where PHP needs 86 MB, and a
  1,000,000-statement file at 896 MB against 141 MB. A `Statement` is 336
  bytes and an `Expression` 96, each in its own box; boxing the large
  statement kinds (`foreach`, `for`, `try`) would shrink both.
- A class with 100,000 properties compiles in linear time but takes minutes
  to link at run time (`crates/vm/src/classes/`).
