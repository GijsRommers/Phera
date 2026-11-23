# Loop cases beyond `Machine::step` go through one out-of-line function

- Date: 2026-11-20
- Area: Speed and method
- Decided by: Design

## Decision

Instructions that the interpreter loop now runs itself but that are not
calls, returns or integer arithmetic (plain value operators, property reads
and writes from their sites, calls by name, class names) are reached
through `Machine::further_step`, one `#[inline(never)]` function. `step`
only lists which instructions go there; every other instruction goes to the
VM without calling it. (`crates/vm/src/calls/further_steps.rs`)

## Why

Measured with fat-LTO builds: a call to the new handler from the inner loop
cost 4 to 12 percent on every benchmark, also those that never reach it;
separate arms in `step` for each case still cost 2 to 5 percent on `fib`,
`hierarchy`, `spl` and `array`. With one listed arm and one out-of-line
function, no benchmark is more than 1 percent slower than before and the
cases themselves avoid a round trip through the VM that costs 250 to 850
machine instructions.

## Turned down

Inline handlers in `step` (layout cost above); handling them in the inner
loop's fallback (worse); a catch-all `_ => further_step` (every remaining
VM instruction then paid for the call; `exceptions` +1.7 percent).
