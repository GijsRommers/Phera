# A readonly initialization during clone-with updates is the VM's

- Date: 2026-11-24
- Area: Interpreter loop, properties, cloning
- Decided by: Design

## Decision

The loop initializes an empty readonly property slot itself (#226). While a clone's `clone with` updates run, PHP lets each readonly property change once, and the VM records which did (`Vm::clone_can_update`). The loop now leaves such a clone to the VM (`CloneScopes::covers`). The clone scopes moved from the VM into `ClassStates`, which the loop already holds.

## Why

`Zend/tests/clone/clone_with_012.phpt` failed since #226: a `set` hook initialized a readonly property in the loop without the record, and the update of the same property that followed in that clone was allowed instead of raising "Cannot modify readonly property". It passes again; nothing else in either php-src tree changes.

## Cost

Requests against main ee0fa9d4a: Symfony +0.03%, hello +0.04%, posts +0.02%. Four benchmarks, which never clone and create no readonly property, are dearer by code layout of the property write: properties +0.29%, typed-objects +0.27%, function-objects +0.29%, reflection +0.12%. Tried and kept no better: the scopes as a field of the loop's `Machine` (up to +0.5%), the initialization out of line, the lookup out of line.

## Structure: was, is

| structure | was | is |
| --- | --- | --- |
| the clone scopes | a field of the VM | a field of `ClassStates`, read by the loop's readonly initialization |
