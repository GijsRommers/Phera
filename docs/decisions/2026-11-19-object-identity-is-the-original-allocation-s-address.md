# Object identity is the original allocation's address

- Date: 2026-11-19
- Area: Speed and method
- Decided by: Design

## Decision

`WeakObject` (weak references, WeakMap keys, a fiber's handle
on itself) recognises an object by the address of the allocation it was
created in (`Object::identity`). The object that waits for its destructor
carries the identity of the one it took over from, recorded with its id in
the pool's incarnations. The id goes back to the pool through `PoolHold`,
the object's last field, so inner objects' ids are freed before the outer
one's, as in PHP. (`crates/value/src/weak_object.rs`,
`crates/value/src/object.rs`)

## Why

A weak handle must still see the object while its destructor runs
(the `Rc` is new then), and must not mistake a later object with the same
id for it. A `Weak` keeps its allocation reserved, so the address is unique
for as long as the handle exists. This keeps `Object` at 128 bytes, the
size of `main`'s: at 144 bytes the allocator took its slow path on most
allocations (about 200 more instructions per object).

## Turned down

A per-object serial number (the first version: 16 more bytes
per object, about 200 more instructions per allocation); keeping the id lease as its own field holding
the id (8 more bytes); giving the id back first in `Drop` (changes the
order in which PHP reuses ids, which `spl_object_id` and `var_dump` show).
