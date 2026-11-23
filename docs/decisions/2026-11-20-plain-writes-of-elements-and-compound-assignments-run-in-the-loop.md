# Plain writes of array elements, compound assignments and increments of elements run in the interpreter loop

- Date: 2026-11-20
- Area: Interpreter loop
- Decided by: Design

## Decision

A write `$a[k1][k2][k3] = v` of a local array or an array property (through the property's site, which the VM records when it makes such a write itself) runs in the loop when it is a plain one: up to three integer or string keys, every level but the last an array already, the last element not a PHP reference. It is one walk that separates each level from other owners (copy on write) and stores the value. A compound assignment (`+=` and the other operators but `.=`) of a declared property or such an element, and `++`/`--` of an element holding an integer or float, read the place as a read does, apply the operator when it raises nothing on plain values, and write back as a plain write does. Everything else (creating levels, references, `ArrayAccess`, hooks, magic methods, readonly, values that do not fit the type, warnings) stays with the VM, and so does the guarded loop.

## Why

The general path detached every level into a parent stack, resolved the property twice and rebuilt the chain: `$o->a['k'] = $i` cost 8,076 machine instructions against PHP's 172, `$o->x += 2` 8,712 against 166. In the loop: 1,093 and 917, within twice the read of the same place.

## Turned down

`.=` in the loop: the VM appends to a string it owns alone in place; doing it here as an operator would copy the string each time.
