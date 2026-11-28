# One-op counted loops run in closed form

- Date: 2026-11-28
- Area: Speed and method
- Decided by: Design

## Decision

A scalar loop that is only a counter and an optional second increment
(`for ($i = 0; $i < $n; $i++) $x++;`, the `IncIncLoop*` and `IncOnlyLoop*`
kernel ops) runs as one step on the registers: the number of times the body
runs is known on entry, so both counters move by it at once. The step charges
the work budget one unit per back-edge and yields at the same iteration the
step-by-step loop would. When an increment would overflow, nothing is written
and the step-by-step loop runs instead, falling back at the exact iteration.

The fused op reads a register limit once, so it is no longer used when the
incremented variable is the limit itself (`for (; $i < $n; $i++) $n++;`),
which the earlier code got wrong.

## Why

`nestedloop` of `Zend/bench.php` enters its innermost 12-iteration loop
248,832 times; setting up the kernel's slot arrays on each entry cost more
than the loop. It went from 5 ms to 3 ms.

## Turned down

- **Closed forms for loops with more ops.** Only the counter-only shape has a
  trip count known on entry without reasoning about the body.
