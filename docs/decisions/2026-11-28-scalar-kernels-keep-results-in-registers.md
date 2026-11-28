# Scalar kernels keep a result in a register for the op that uses it

- Date: 2026-11-28
- Area: Speed and method
- Decided by: Design

## Decision

The unboxed scalar-loop kernel (`fast_tier/scalar_loop.rs`) fuses an op with
the op that consumes its result, so the value stays in a register instead of
going through the slot array:

- `y = x * x` right after the op that made `x` folds into that op
  (`AddSqrF64`, `SubAddSqrF64`, ...: the op writes `x` and its square).
- Two products combined by an add or subtract, optionally plus a third
  value, are one op: `a*b + c*d`, `a*b - c*d`, `(a*b - c*d) + e`.

Every fused op computes the same IEEE operations in the same order as the
separate PHP operations, so results are bit-for-bit PHP's. The rewrite of
`$x * $y * 2 + $z` into `($x + $x) * $y + $z` was not: it overflows to `INF`
when `$x + $x` does (`$x = 1e308, $y = 1e-300`) and rounds differently when
the product is subnormal. It now doubles the product (`MulDblAddF64`).

## Why

A floor experiment (a standalone copy of the kernel on the `mandel` loop)
showed that the dispatch form costs little (a `for` over the ops instead of
an `ip` loop: about 13%) and the store and reload of each result most: the
loop's values go through memory on every op, and that chain sets the speed.
Folding `$re2 = $re * $re` into the op before it ran the loop in 7.5 ms
instead of 17.3 ms. In `Zend/bench.php` on an M-series Mac, `mandel` went from
15 ms to 9 ms and `mandel2` from 19 ms to 14 ms.

## Turned down

- **An accumulator register any op may read.** General, but selecting the
  operand costs a compare per operand: 10.9 ms in the same experiment.
- **One kernel for the whole `z = z² + c` iteration.** Fastest, but it only
  fits escape-time fractal loops: a benchmark shape, not a general
  optimisation.
- **Pairs of independent ops per dispatch.** 9.4 ms, and it does not shorten
  the chain through memory.
