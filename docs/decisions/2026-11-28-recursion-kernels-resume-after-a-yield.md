# Recursion kernels take their frames back after a yield

- Date: 2026-11-28
- Area: Speed and method
- Decided by: Design

## Decision

The specialised integer recursion kernels (`fast_tier/int_rec.rs`: the
`fibo`-shaped `UnaryBinOp` and the `Ack`-shaped `BinaryNested`) still hand
their frames to the VM when the work budget runs out, so the request yields
as before. When the machine later continues a frame of such a function at
the instruction after one of its self-calls, the fast tier walks down the
frames of the same function waiting at such calls, reads the few integers
each one keeps (`$n`, the first call's result, the outer call's `$m`), and
continues the recursion in the kernel. Any other frame, or a register that
does not hold an int, ends the walk.

The nested kernel keeps its waiting frames in a growable scratch stack up to
the request's frame limit instead of a fixed 2,048-entry array zeroed on every
call, and keeps each waiting frame's `$n`, so a materialised frame shows its
real argument in a stack trace instead of 0.

## Why

A kernel runs at most `65,536 × 8` steps between yields. `fibo(30)` (2.7
million calls) and `Ack(3, 7)` (entered with part of the budget spent) used
to finish frame by frame after the first yield, at about 60 ns a call:
`Ack(3, 7)` took 0.56 ms alone and 7 ms inside `Zend/bench.php`, and
`Ack(3, 8)`, deeper than the fixed array, took 172 ms (PHP: 35 ms). Now
`ackermann(7)` takes 1 ms, `fibo(30)` 4 ms instead of 7 ms, and `Ack(3, 8)`
2 ms.

## Turned down

- **Keeping the kernel's state across the yield.** The scheduler resumes a
  request from its VM frames; a second representation of a suspended
  recursion would have to be kept exact for every other path that reads
  frames (traces, errors, debugging).
- **A larger budget per kernel entry.** It would lengthen the time between
  yields, which other requests on the worker wait for.
