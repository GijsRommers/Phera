# Release register windows with empty trailing slots

Decided by: Design

## Decision

Keep the existing forward release. Skipping groups of four empty trailing
registers raised the cost of both Laravel routes on two sets of six pairs.

## Trial

On a PHP frame exit, the candidate shortens the window by groups of four
consecutive trailing slots already known to be undefined. It releases the
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

## Structure under test

| Structure | Parent | Candidate |
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

## Measurements

All performance comparisons used normal release binaries on Apple silicon.
The parent was `phera-main-f9cf1e1f`, the candidate was `bb56bc36`, and both
used the macOS 26.2 SDK and linker 1230.1. The parent and candidate ran
alternately through the shared quiet gate. Shape costs are the lowest of
five slopes from one million to 51 million iterations.

| Shape | Parent instructions/iteration | Candidate | Difference | Cycles/iteration, parent → candidate |
|---|---:|---:|---:|---:|
| Loop | 146.192 | 146.190 | -0.002 | 20.23 → 20.43 |
| Function | 654.387 | 680.402 | +26.015 | 80.34 → 83.42 |
| Method | 830.411 | 855.411 | +25.000 | 116.64 → 121.28 |
| Property | 593.197 | 593.203 | +0.006 | 79.28 → 79.40 |

In the parent, the compiler inlines the forward release into the interpreter
loop. The candidate's loop calls a separate helper. On arm64 that helper
saves four pairs of registers, computes slice bounds and tests the window
length before it can skip any slots. For each complete group it then
checks four tags separately. Short windows pay the entry cost even when
they have no group to skip. This accounts for a cost the empty-slot census
could not measure.

| Route | Parent M instructions/request | Candidate M | Change | Cycles/request, parent → candidate (M) |
|---|---:|---:|---:|---:|
| `/api/hello` | 20.887 | 20.943 | +0.271% | 11.96 → 12.20 |
| `/api/posts?page=2` | 50.714 | 50.781 | +0.131% | 22.04 → 20.95 |
| `/api/db-yardstick` | 265.580 | 265.460 | -0.045% | 83.99 → 75.30 |

The first six Laravel pairs gave +0.297% for hello and +0.172% for posts.
The pair spread exceeded 0.3 percentage points, so both routes were run
again. The table gives the repeat using the unrounded instruction counters.
Symfony used two alternating pairs with 300 warm-up requests and matched
the response digest in every run. Its 0.12M saving per request was a 0.045% decrease. Both Laravel routes rose past the 0.1% limit.

Each benchmark ran five times per binary. All ten exceeded the 0.1% limit
at both the lowest and the median instruction count.

| Benchmark | Parent low / median (M) | Candidate low / median (M) | Low change | Median change |
|---|---:|---:|---:|---:|
| fib | 1,654.8 / 1,655.1 | 1,727.7 / 1,728.0 | +4.401% | +4.399% |
| calls | 5,195.1 / 5,195.2 | 5,295.6 / 5,295.9 | +1.934% | +1.939% |
| methods | 16,522.9 / 16,523.1 | 17,023.3 / 17,023.4 | +3.028% | +3.028% |
| properties | 2,105.8 / 2,105.9 | 2,122.3 / 2,122.4 | +0.786% | +0.785% |
| hierarchy | 2,128.3 / 2,128.4 | 2,180.2 / 2,180.5 | +2.441% | +2.447% |
| typed-objects | 5,418.9 / 5,419.1 | 5,455.9 / 5,456.2 | +0.682% | +0.685% |
| objects | 3,526.6 / 3,595.7 | 3,606.9 / 3,676.0 | +2.278% | +2.233% |
| function-objects | 3,602.4 / 3,602.8 | 3,632.6 / 3,674.1 | +0.837% | +1.978% |
| closures | 4,206.1 / 4,206.2 | 4,257.5 / 4,257.7 | +1.222% | +1.226% |
| framework-patterns | 7,398.8 / 7,400.7 | 7,439.7 / 7,443.2 | +0.552% | +0.575% |
