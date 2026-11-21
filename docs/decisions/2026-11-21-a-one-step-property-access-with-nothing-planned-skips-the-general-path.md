# A one-step property access whose class plans nothing for it is done without the general path

- Date: 2026-11-21
- Area: Properties
- Decided by: Design

## Decision

Some property accesses leave the interpreter loop:
- `$o->$name` read or written;
- `unset($o->p)`;
- `isset`, `??` and `empty` on a property without a value.

Such an access first reads the class's `PropertyFacts` word, before the general asynchronous property path. If the access has one property step, the object is not lazy, and none of the flags that matter to the access are set, the VM does it at once:
- a read or a quiet fetch takes the property's value from its slot or from the undeclared table. A quiet fetch gives null when there is no value.
- a write stores the value, as is or converted the way PHP converts a scalar without running code. A declared property's slot is written, and an undeclared one is made where PHP makes it without a deprecation.
- an unset empties the declared slot or removes the undeclared key.

The flags that matter:

| access | flags it avoids |
| --- | --- |
| read | `__get`, hooks |
| quiet fetch | also `__isset` |
| write | `__set`, hooks, readonly |
| unset | `__unset`, hooks, readonly |

Everything that warns, throws or runs PHP code keeps the general path:
- an undefined or uninitialized plain read;
- a TypeError;
- an object written to a property of a class type, or to a string type (`__toString`);
- asymmetric visibility;
- a PHP reference in the property;
- a name that a static property of the class has;
- a deprecated dynamic creation.

`isset` and `??` try the cached getter first, as before, so a declared property that a site already reads costs what it did.

An assignment gives the value that was stored. With `AssignPathFromLocal` and the property site, that is the converted value (`1` written to a `float` property gives `1.0`).

## Why

These accesses each took the general path: 5,000 to 8,000 machine instructions against PHP's 70 to 280.

| access | before | after | PHP |
| --- | ---: | ---: | ---: |
| `$h->$name = $v` | 7,727 | 1,986 | 169 |
| `$h->$name` read | 5,257 | 1,919 | 141 |
| set and `unset` of a dynamic property | 7,481 | 2,332 | 281 |
| `unset` of a declared untyped property | 6,419 | 1,648 | 208 |
| `isset` of an uninitialized typed property | 5,978 | 2,799 | 71 |
| `??` on an uninitialized typed property | 5,617 | 2,443 | 132 |

What remains is mostly the trip out of the loop and back (about 640), the loop arms tried before the exit, and the property key lookup. The site work that keeps these in the loop comes later.

## Turned down

- Planning in the general path with the flag word: it is still asynchronous and still takes the trip, and the case with nothing to plan needs no plan.
- Doing chained paths (`$h->cls->id`) here: those come in their own change, with the intermediate object's facts.
