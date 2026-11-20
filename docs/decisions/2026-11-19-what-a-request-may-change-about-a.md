# What a request may change about a class lives outside the class

- Date: 2026-11-19
- Area: Speed and method
- Decided by: Design

## Decision

A linked class holds no state a script can change. Static
properties, the values of class constants evaluated on first read, and the
flags that say whether either was initialised live in tables of the request
(`ClassStates`), reached through slot numbers the class keeps; a child that
inherits a static shares its parent's slot. The method tables used by fast
calls move from the class into the request's class table, because they are
keyed by the request's own name numbers. (`crates/vm/src/classes/class_state.rs`,
`crates/vm/src/classes/class_table.rs`)

## Why

A snapshot and every request forked from it share one `Rc<Class>`
per class: copying 500 classes per request would cost what the snapshot is
meant to save, and a shared class must not carry one request's static
values into another. It is also the first half of PHP's inheritance cache.

## Turned down

Copying classes in every fork (cost per request, and parent
pointers would need remapping); keeping method tables in the class (a table
built in one fork after it numbered a new name could map that number to the
wrong method in another fork).
