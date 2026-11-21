# Keep small array drops off the walker

- Date: 2026-11-21
- Area: Speed and method
- Decided by: Design

## Decision

Let Rust drop unindexed maps whose values cannot lead to nested array release.
Keep the one-pass release for lists, indexed maps, and small maps that contain
arrays, objects or references. Put that walker out of line so it takes no
stack space until a drop needs it.

## Why

The previous release build reserved 224 bytes in `PhpArray::drop` before
checking its storage. The fast path now uses no stack frame. The small-map
change alone lowered `bench-objects` by 0.60%, and moving the walker out of
line brought the total reduction to 0.77%. The five affected benchmarks all
retired fewer instructions. In the shared probe, `fill64` went from 3,497 to
3,462 instructions and the destructor case went from 5,738 to 5,675.

## Turned down

Scanning every array before release would reintroduce a full extra pass for
larger arrays. The scan stays on the unindexed maps used for small property
tables.
