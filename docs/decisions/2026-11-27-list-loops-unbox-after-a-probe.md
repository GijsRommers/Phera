# List loops unbox after a probe, and bulk adds resume

- Date: 2026-11-27
- Area: Speed and method
- Decided by: Design

Extends `2026-11-27-list-loops-choose-their-element-layout.md`.

## Decision

- A Tier-2 list loop that writes its array and starts in place (its previous
  entry ran fewer steps than the array has slots) runs in place for at most
  one step per slot. If it has not finished by then, it unboxes the array and
  goes on from the list op where it stopped, with the same registers; a hole
  or another element type keeps it in place for the rest of its budget.
- An unboxed run gets eight steps per step of the caller's budget and is
  charged at that rate, as `int_rec` is.
- A bulk `foreach ($src as $k => $v) $dst[$k] += $v;` run that spent its
  budget marks the iterator (`IteratorState::bulk_resume`) and goes on in
  bulk after the entry it stopped at; a loop that left the bulk run for a
  missing key or another type stays on the general path. A bulk run gets
  four entries per step of the budget.

## Why

The first `heapsort_r` call of `Zend/bench.php` ran its 65,536-step budget in
place, then yielded and was unboxed and copied back at every one of about ten
later budgets. The bulk add of `hash2` ran its first budget in bulk and the
rest of that `foreach` one entry at a time.

## Turned down

- **Unboxing at every entry.** Keeps the O(length) entry for searches, which
  the in-place layout was made for.
