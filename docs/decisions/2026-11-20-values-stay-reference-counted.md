# Values stay reference counted; no arena per request

- Date: 2026-11-20
- Area: Speed and method
- Decided by: Design

## Decision

A PHP value stays what it is today: a 16-byte `Value` whose strings, arrays,
objects and references are owned through `Rc`. Phera does not move values
into tables owned by the request ("an arena per request", "values as
handles"), and does not skip the release of values at the end of a request.

The memory work of a request is made smaller on the present design instead:

1. An array literal of constants is taken from the unit's constant table and
   separated on the first write. Any other literal is built by one
   instruction that knows whether it is a list or a map and how many
   elements it has.
2. Declared properties live in slots in class order, with the names kept
   once in the class. Properties that were not declared live in a side table.
3. A string of up to 14 bytes is stored inside the value, without an
   allocation.
4. Built-in functions create an array at its final size, and an array is
   released in one pass.
5. The table that hands an object over to its destructor is a flat list
   indexed by object id.

## Why

Two independent studies measured the same thing on the forked Laravel
request with two database queries (168M instructions; PHP with opcache
27.3M):

| What | Instructions |
|---|---:|
| Everything after the last shutdown callback began | 4.55M |
| Release of what is still alive when the request ends | under 1M |
| Allocator and release over the whole request | about 26M |
| The interpreter's general path and everything else | about 106M |

PHP programs observe a count on every array, object and reference: a
destructor runs at the last release, a shared array is copied on the first
write, and an object id is handed out again once its object is gone. Two
aliases inside one function are enough to see all three, so "count only the
values that leave the request" is not possible. A release during the request
therefore stays, whatever owns the memory, and the release at the end is too
small to matter.

The gap to PHP is mostly outside the value layer. Per operation, above an
empty loop:

| Operation | Phera | PHP 8.5 |
|---|---:|---:|
| `[1, 2, 3, 4]` | 3,439 | 61 |
| `[$i, $i + 1, $i + 2, $i + 3]` | 3,670 | 588 |
| `$t[0] = $i` on an array nobody shares | 2,268 | 95 |
| `$r = &$x; unset($r);` | 4,213 | 181 |
| `new P` with three declared properties, one read, release | 1,636 | 482 |
| `'row ' . $i` | 1,314 | 597 |

Between 11% and 48% of each of these is allocation, counting, copying and
release; the rest is how the interpreter builds and reaches the value. Two
measured controls show the size of point 1 alone: 1,675 instructions fewer
for a list of four, and 3,096 fewer for a constant list that is copied and
appended to.

## Turned down

- Values as indexes into tables of the request, with the counts in the
  tables. In safe Rust a handle cannot change a count when it is cloned or
  dropped unless it can reach its table, so every use becomes an explicit
  share or release (about 1,100 files name `Value`), or goes through a
  thread-local "current request". A missed release is then a missed
  destructor that the compiler cannot find. The estimate is 20,000 to 40,000
  changed lines for a few million instructions more than the points above.
- A bump arena that is thrown away at the end of the request. It never reuses
  memory inside a request, so a queue worker or a long loop grows without
  bound, and values that borrow from the arena cannot be held by a task
  without `unsafe` in a helper crate.
- Skipping the release of values without a destructor at the end of a
  request. Without leaking the memory it saves nothing, and it is under 1M.
- A use of `unsafe` for a refcounted string or array in one allocation.
  Making all memory work free would give at most 1.33 times on the probe,
  far from the factor of ten the owner asks before `unsafe` is proposed.
