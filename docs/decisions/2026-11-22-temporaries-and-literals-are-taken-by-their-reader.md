# Temporaries and literals are taken by their one reader

- Date: 2026-11-22
- Area: Speed and method
- Decided by: Claude Opus 5.5 (call protocol and loop core), reviewed by Fable

## Decision

A late compiler pass (`crates/compiler/src/passes/takes.rs`), run after every
other pass, marks the places where a temporary has exactly one reader:

- `AssignTake { local, src }` and `SendTake { src }`: an `Assign` or
  `SendValue` of a temporary followed by its `Clear`, with nothing between
  that touches it and no jump into the stretch. The value moves; the `Clear`
  goes.
- `AssignConst { local, constant }` and `SendConst { constant }`: a
  `LoadConst` into a temporary whose one reader is one of the above, or a
  plain `Assign`/`SendValue` whose temporary `dead_after` proves unread. The
  literal is read in place; the `LoadConst` goes.
- `MethodSite.receiver_taken`: a temporary receiver whose `Clear` follows the
  site's `Call`. The `Clear` moves before the `Call` (the prepared call holds
  `$this` by then), and a quick entry moves the receiver into `$this` instead
  of copying it. `after` keeps its meaning, so traces and unwinding still
  read the caller's `ip - 1` as the `Call`.

## Why

The owner asked for the value shuffle bytecode to go. On the Symfony route
a request executes about 106,500 `Clear`s, 38,000 `LoadConst`s and 38,000
`Move`s of 430,700 instructions. Of the shares Fable counted, the
method receiver's `Clear` is 22.7%, an `Assign` of a temporary 10.6% and a
send of one 4.9%; a `LoadConst` feeding a `SendValue` is 9.6% and one
feeding an `Assign` 9.1%. Each is a dispatch, and each copy is a reference-count increment that a `Clear`
undoes.

Destructor order follows PHP's: a temporary receiver is released when the
callee returns, before its result is used, as PHP releases `$this` on leaving
the call (fixture `tests/fixtures/calls/temporaries-taken-by-their-reader.phpt`).

## Turned down

- A "take" bit in every `Assign` and `Send`: a test on every execution.
- "Temporaries above the locals are always taken": free to decode, but any
  double read would become a silent behaviour change.
- Resuming a taken site one past the receiver's `Clear`: traces and
  exception unwinding read the caller's `ip - 1` as the `Call`, so that
  instruction must stay the last one before `after`.
