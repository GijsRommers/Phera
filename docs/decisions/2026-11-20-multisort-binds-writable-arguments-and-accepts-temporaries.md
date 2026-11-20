# Multisort binds writable arguments and accepts temporaries

- Date: 2026-11-20
- Area: Scope and compatibility
- Decided by: Design

## Decision

Model a preferred reference independently from a required reference. A
variable supplies its reference cell, and a temporary is accepted by value.
`array_multisort()` compares snapshots column by column in Zend sort order
and writes the sorted rows back to the original arrays.

## Why

A multisort call can mix writable arrays with literal sort flags. PHP's
Reflection also reports that these parameters can be passed either way.
The send path distinguishes the two reference modes only when it must
decide whether to accept a value in a reference position.

## Turned down

Treating flags as required reference arguments or keeping variable
references across a PHP comparison callback.
