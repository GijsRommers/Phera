# Keep popped slots for live array cursors

- Date: 2026-11-20
- Area: Value layer
- Decided by: Design

## Decision

Leave trailing slot reuse to the interpreter's array cursor work. Array
storage has no marker for a live by-reference cursor.

## Why

With `$a = [1, 2]`, a by-reference `foreach` that pops the entry holding
`2` and appends `3` visits `1, 2, 3` on PHP. Phera's cursor remembers the
last slot it visited and starts after it. Reusing that physical slot would
skip `3`.

## Turned down

Adding cursor tracking to arrays as part of the slice, seek, and merge
optimizations. Tail reuse needs an iterator that can recognize when its
current slot was removed and reused.
