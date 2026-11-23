# Declared properties live in slots

- Date: 2026-11-21
- Area: Engine, objects
- Decided by: Claude (design and implementation), approved by Fable

## Decision

- A class owns the layout of its declared instance properties: their keys once, in declaration order, parents first, in a flat list (`PropertyLayout`). The key is the name PHP stores: the plain name for a public property, `\0*\0name` for a protected one, `\0Class\0name` for a private one.
- An object holds one slot per key of its class's layout, and nothing where a property is unset or not yet initialized. Properties the class does not declare (and every property of a class without a layout, such as `stdClass`) are kept in a table of their own, in insertion order, made on the first one.
- A property site, a `ReflectionProperty` and the class's defaults resolve a property to its slot number when they are linked. They read and write the slot by number, with no name looked up and no key compared.
- Code that needs every property (`(array)`, `foreach` over an object, `get_object_vars`, `var_dump`, `serialize`, `json_encode`, `==`) goes through one view (`Properties`), which walks the slots in layout order and then the other properties.
- `new` copies the class's default values into the slots in one allocation. `clone` copies the slots.
- An object whose table is replaced in an order the slots cannot keep (`ArrayObject::ksort` on an object) leaves the layout: its properties are then all kept by key, and sites and `ReflectionProperty` take the general path for it.
- `ReflectionProperty::isInitialized`, `getValue`, `getRawValue`, `setValue`, `setRawValue`, `setRawValueWithoutLazyInitialization`, `isLazy` and `ReflectionClass::isUninitializedLazyObject` answer from the slot, where the call is, for an object that is not lazy and a property without hooks. Every other case runs the method as before.

## Why

Before, an object's properties were one hash table keyed by the mangled names. `new` cloned the class's default table with its keys and released it again; a site remembered a position in that table and compared the stored key on every access, because `unset` followed by a new write moved the entry to the end; and that move is also why the order of properties differed from PHP's after `unset($o->a); $o->x = 9; $o->a = 1;` (PHP lists `a b c x`).

With the keys in the class and the values in slots, the declared order is PHP's by construction, and the common accesses need no name.

Measured against main `8732c15a`, instructions per request: Laravel `/api/hello` 33.57M to 33.27M (-0.9%), `/api/posts?page=2` 135.97M to 130.03M (-4.4%), the Symfony and Doctrine route 548.92M to 532.74M (-2.9%). The Reflection calls Doctrine makes while hydrating went from 2,152 to 5,118 instructions each to 1,317 to 1,723 (PHP: 279 to 483).

A property read through a site moved only from 495 to 479 instructions (PHP 51): the table probe and the key compare were a small part of it. What surrounds the slot is the next piece of work.

## Known, and left for later

- Finding a slot by key is a scan over the layout's keys. It is used where no slot number is at hand (the general property path). For a class with many properties this wants a second flat table, sorted, for a binary search, if it stays on a hot path.
- `==` on two objects builds an array of each object's properties for the comparison.

## Turned down

- Keeping the hash table and making the remembered position stable. The order after `unset` would stay wrong, and every access would still compare a key.
- A slot number per property found by name at each access. That keeps the lookup this change removes.
