# The loop reads and writes properties from their sites and calls protected methods of related classes

- Date: 2026-11-20
- Area: Speed and method
- Decided by: Design

## Decision

When an instruction's property site (made by the VM the first time it runs)
matches the object's class and the caller's scope, the loop reads the
property (`$this->x`, `$o->x`, `$items[$i]->x`), writes it when the value
fits the declared type as it is, and increments an integer property.
Quiet reads (`??`, `isset`) skip properties with hooks, because PHP calls
the get hook first there. Writes are left to the VM in the guarded loop.
A protected method is prepared in the loop when the caller's scope and the
declaring class are on one parent chain; PHP's prototype rule allows more,
and those cases keep the general check. (`crates/vm/src/calls/fast_properties.rs`,
`crates/vm/src/calls/fast_methods.rs`)

## Why

Property access never stayed in the loop, and protected calls, common in
framework code, never did either. The site already holds the VM's
visibility decision and the storage slot; the loop reuses it rather than
deciding anything itself. The parent-chain test only accepts calls that
PHP's rule accepts.

## Turned down

A second, loop-owned cache (two records of one decision). (Sites for
`$this->items[$k]` reads were first left out; see
`2026-11-20-the-vm-records-sites-for-property-element-reads.md`.)
