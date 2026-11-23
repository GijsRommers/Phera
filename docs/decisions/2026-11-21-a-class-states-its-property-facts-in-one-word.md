# A class states what its property accesses may involve in one word, and a site writes undeclared properties by it

- Date: 2026-11-21
- Area: Properties
- Decided by: Design

## Decision

Each class has a `PropertyFacts` word, made from the linked class on first use. Its flags:
- `__get`, `__set`, `__isset`, `__unset`;
- hooks: some declared property has hooks, or native code supplies the properties;
- readonly: some declared property is readonly, or the class is;
- dynamic properties allowed without a deprecation (`stdClass`, `#[AllowDynamicProperties]`, not a readonly class).

A property access reads this word first. When none of the flags that matter to it is set, there is nothing to plan.

The first user is the property site's write of a property the class does not declare. When the class has no `__set`, no hooks and nothing readonly, the write is done in the loop:
- an existing undeclared property is replaced in its table;
- a missing one is made only where PHP makes it without a deprecation.

A deprecated creation, a PHP reference that must be written through, a lazy object and every flagged class keep the general path.

## Why

Such a write took the VM's general asynchronous property mutation: about 8,000 machine instructions against PHP's 103 to 108. It is every `json_decode` result and every database row fetched as an object.
- **Now:** 557 to 584, the cost of a declared untyped property's write.
- **Next:** the same word lets the general path itself skip the planning for magic methods and hooks for a class that has none. That is the next pull requests' work (`$h->$name`, `unset`, `isset`, chained reads).

## Turned down

A cache of property plans per site: the plan for the common case is "no plan", which the word says without remembering anything.
