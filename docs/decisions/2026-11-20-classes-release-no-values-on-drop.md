# Classes release no values when their table is dropped

- Date: 2026-11-20
- Area: Speed and method
- Decided by: Design

## Decision

`Class::release_values` (batch 14), which emptied a class's static
properties and evaluated constants when a request's class table was
dropped, is removed. Those values live in the request's `ClassStates`, and
are dropped with it. (`crates/vm/src/classes/class_table.rs`)

## Why

The release broke a cycle in which a class owned, through a static, an
object that owned the class. With request state outside the class there is
no such cycle. And the classes of a snapshot are shared by every fork:
releasing them when one fork ends would empty them for the others.

## Turned down

Keeping the release for classes that are not shared (two kinds of class
for one purpose).
