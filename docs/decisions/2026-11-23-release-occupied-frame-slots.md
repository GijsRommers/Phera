# Release register windows with empty trailing slots

## Decision under test

When a PHP frame leaves the stack, shorten its window by groups of four
consecutive trailing slots already known to be undefined. Release the
remaining slots from first to last. On ordered returns the receiver and
closure are still held until after the locals. Native argument windows
use their existing truncation.

## Ledger

The earlier 453M Symfony profile charged 22.85M instructions to 38,378 loop
returns, including 11.03M in the arm. The remainder includes live-value
release. The parent for this trial is main `f9cf1e1f`. A separate counting
build observed the slots immediately before each return. One Symfony worker
served 303 warm-up requests, then ten checked requests from its snapshot.
The averages below describe successful returns; exception unwinding and
shutdown are excluded from the census.

| Return path | Frames/request | Slots scanned | Trailing empty slots | Entirely empty windows |
|---|---:|---:|---:|---:|
| plain | 36,772 | 504,988 | 273,014 | 1 |
| ordered | 1,456 | 24,888 | 10,776 | 0 |
| callback | 0.9 | 4.5 | 2.7 | 0 |
| native loop | 0 | 0 | 0 | 0 |
| discarded | 0 | 0 | 0 | 0 |
| VM | 586 | 6,005.2 | 1,715.4 | 11 |

| Value at release | Receiver / closure | Named locals | Remaining slots | Total |
|---|---:|---:|---:|---:|
| undefined | 1,271 | 26,646.5 | 363,192.4 | 391,109.9 |
| null | 0 | 1,314 | 4,114.9 | 5,428.9 |
| bool | 0 | 2,160 | 11,792 | 13,952 |
| int | 0 | 3,483 | 4,981 | 8,464 |
| float | 0 | 1 | 0 | 1 |
| string · one | 0 | 446 | 20 | 466 |
| string · shared | 0 | 4,075 | 5,107 | 9,182 |
| array · one | 0 | 639 | 2,084 | 2,723 |
| array · shared | 0 | 3,919 | 2,278 | 6,197 |
| object · one | 8 | 1,720 | 0 | 1,728 |
| object · shared | 37,595.9 | 29,379 | 5,799 | 72,773.9 |
| resource · one | 0 | 0 | 0 | 0 |
| resource · shared | 0 | 0 | 0 | 0 |
| reference · one | 0 | 74 | 0 | 74 |
| reference · shared | 0 | 137 | 11 | 148 |
| interned string | 0 | 16,265 | 7,373 | 23,638 |


Each window has an optional receiver/closure prefix, then named locals and
temporaries. Undefined slots need no write. Occupied scalars, including
interned strings, must become undefined before the next call can reuse them.
Owned strings, arrays, objects, resources and PHP references each release a
handle. The sole-owner columns count handles before any drops, so two slots
sharing the last two handles appear as shared twice.

Of the 535,886 slots scanned per request, 391,110 were empty. Consecutive
empty tails accounted for 285,508. The baseline machine code reads each
tag, calls value drop glue for occupied slots and stores an undefined tag
even on slots it found empty. Groups of four can skip the forward scan
over at least 169,000 slots per request, allowing for three unskipped
tail slots in every frame. Route gates measure the net benefit of inspecting
the tails first.

## Structure

| Structure | Was | Is |
|---|---|---|
| Frame release | Scan and overwrite every slot | Skip complete groups of four empty trailing slots, then release the remainder in order |
| Register writes and compiled bytecode | Store optional PHP values in the frame | Same representation and writes |
| Ordered release | Retain the receiver and closure while locals are dropped | Same order after shortening the window |

## Alternatives

A last-use analysis can prove some temporaries empty. Conditional assignment,
exception handlers and dynamic locals can leave different slots occupied at
the same return site. Elided clears can leave dead scalars in those slots.
A per-write bitmap or high-water mark would add bookkeeping to every
assignment. The trial tests the existing tags once per frame exit.
