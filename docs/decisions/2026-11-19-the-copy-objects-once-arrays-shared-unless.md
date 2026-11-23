# The copy: objects once, arrays shared unless they hold identity

- Date: 2026-11-19
- Area: Speed and method
- Decided by: Design

## Decision

Every object and reference cell reachable from the VM is
copied once (same id, same class), so sharing and cycles stay; an object is
made empty and queued, then filled, so no copy recurses over the data. An
array is shared unless it holds an object, a reference or a resource
somewhere inside; then a clone of it is rebuilt (keeping its internal
pointer and next free key). Native state goes through a fork hook per class
(`ClassHooks::fork`), open resources through a hook per kind
(`Registry::fork_resource`); a class or resource without one refuses.
(`crates/value/src/fork/`)

## Why

That is architecture 5.4's contract. Sharing scalar arrays keeps
the copy proportional to the objects, which is what a booted framework
mostly holds.

## Turned down

Copy on first write (step 4 of 5.4): a change to the value
representation, to be proposed only if the copy shows in profiles.
