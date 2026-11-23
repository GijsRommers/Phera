# The generators benchmark may be 1.1 percent slower in instructions in interpreter round three, part one

- Date: 2026-11-21
- Area: Speed and method
- Decided by: Design

## Decision

Interpreter round three, part one (native calls from the batch loop, callbacks
of built-ins on the frame stack, Reflection members by position) merges with
one benchmark over the 1 percent rule: `bench-generators.php` retires 1.0 to
1.1 percent more instructions than main (5,576M to 5,635M). No other script
is over the rule; the nearest is `bench-fib.php` at +0.9 percent.

## Why

The rise is where the compiler places code, not work that was added:
`Vm::set` and the call handler are no longer inlined into the generator's
`foreach` step once the batch loop has the arm for native calls. On the same
five runs the script needs 7.6 percent fewer cycles (1,120M to 1,032M), so it
finishes sooner. The same change takes 4.8 percent off the plain Laravel
request and 10.1 percent off the database request, and 18 of the 20 other
scripts retire fewer instructions (closures 15.7 percent, reflection 11.8
percent).

The 1 percent rule exists because small costs in the loop add up unseen.
This one is seen, named, and has its cause recorded; the next change to the
loop is measured against the new number.

## Turned down

- `#[inline(always)]` on `Vm::set`: the script then retires 3.0 percent more.
- A direct register write in the generator's `foreach` step: no change.
- Holding the change back for this one script: it would keep 15M
  instructions on every database request to save 59M on a script of 5,576M.
