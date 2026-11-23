# The trip out of the scalar loop stays; the loop-core targets come from measured pieces

- Date: 2026-11-22
- Area: Speed and method
- Decided by: Design (Claude Opus 5.5, call protocol and loop core, with Fable); the owner can overrule it

## Decision

The interpreter keeps its present structure: `run_simple` runs the scalar
instructions with the frame's register window borrowed and leaves for every
other instruction, which `step` (inlined into `Machine::run`) takes. Calls and
returns are not moved into the scalar loop, and nothing is called from inside
it. Speed work on calls and returns goes into the handlers that `step` reaches.

The definition of done in `docs/architecture.md` §10.14 changes from "loop at
most 90, function call at most 290, method call at most 360" to "loop at most
165, function call at most 620, method call at most 750", built from the
measured pieces of a method call and revised as each piece lands.

## Why

Five ways of removing the trip were built and gated on the Laravel routes at
twelve pairs against main; every one raised them (+1.07% to +3.33%), and the
last, two small out-of-line handlers called from the loop, showed why: a loop
that executes none of the handled instructions paid 16 instructions more per
iteration, because any call inside the scalar loop makes it compile worse
under fat LTO. The handlers saved about 4 per execution; the scalar
instructions, which are about half of a request, paid more. The tables, the
patches and the disassembly are in `~/.phera-review/floor/LOOP-PLAN.md`.

## Turned down

- Calls and returns in a frame loop inside `run_simple`, with or without the
  switch out of line; `run_simple` out of line with calls dispatched in
  `run`: +1.8% to +3.3% on the routes.
- The whole second dispatch called from inside the loop: its function saves
  twelve registers and 400 bytes of stack per call, about 50 instructions a
  call-out.
- Small per-instruction handlers called from the loop: cheaper per call, but
  the scalar loop around them costs more.
