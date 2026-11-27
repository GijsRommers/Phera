# Tier-2 list loops choose their element layout per entry

- Date: 2026-11-27
- Area: Speed and method
- Decided by: Design

## Decision

A Tier-2 `ListLoop` (an inner loop whose only heap accesses are integer
indexes into one packed array) runs on one of two layouts, chosen when it is
entered:

- **In place**, on the array's own `[Option<Value>]` slots: each read checks
  that the slot holds the element type, each write that it holds a plain int
  or float, and a mismatch falls back to FastVm before that operation.
- **Unboxed**, on a flat `[i64]` or `[f64]` copy made at entry and stored back
  at exit, number by number in place (the old copy-back assigned whole
  `Value`s and ran the destructor glue for each element).

The loop's previous entry decides: if it ran at least one step (backward
branch or fill store) per slot of the array, the next entry unboxes; else it
works in place. A loop that only reads the array no longer separates it from
other holders (`Rc::make_mut`): it borrows the shared slots.

## Why

Before, every entry copied the whole array in and out, so a loop entered often
on a large array paid O(length) per entry, and a read-only loop on an array
passed by value cloned it first. A binary search over a million ints, called
20,000 times, took 42 ms (PHP 4.9 ms); now 11.7 ms. In place costs a few
instructions per access, which a sort (many accesses per element) notices:
the heap sort of `Zend/bench.php` keeps its old cost only with the unboxed
layout. Measured, instructions (lowest of three): `heapsort` 2,212M before,
2,219M after; `sieve` 744M before, 631M after (cycles down by about 36%).

## Turned down

- **Always in place.** `heapsort` +12% instructions.
- **Always unboxed (with the cheaper copy-back).** As fast as the choice on
  sorts and sieves, but keeps the O(length) entry for searches.
- **A fixed size threshold.** The array's length alone does not say how much
  work an entry does; the previous entry of the same loop does.

## Structure: was, is

| Structure | Was | Is |
| --- | --- | --- |
| Elements during a list loop | always an unboxed `Vec<E>` copy | the array's slots, or the unboxed copy when the last entry ran ≥ 1 step per slot |
| Copy-back | `*slot = Some(value)` (destructor glue per element) | `E::store`: the number is updated in place |
| Read-only loop on a shared array | `Rc::make_mut` clones it | borrows it |
| `FastCandidates::Large` | `Rc<[bool]>` | `Rc<[u64]>` bitset |
