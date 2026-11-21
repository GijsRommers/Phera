# A property site checks an object written to a class-typed property by the object's ancestry

- Date: 2026-11-21
- Area: Properties
- Decided by: Design

## Decision

When a property site's entry is made for a declared property whose type names classes, the entry resolves those classes once:
- `self` and `parent` from the declaring class;
- the others by name;
- a plain intersection as "all of them".

A write of an object then stores it in the loop when the object's class is one of those classes, or extends or implements one. The check walks the linked ancestry by pointer, as `instanceof` does.

These keep the full check:
- a type the entry cannot resolve (a class the request has not declared yet, a DNF type, `static`);
- a failed check;
- a coercion (a `Stringable` object into `string`).

The full check now resolves `parent` too, and its TypeError names the classes `self` and `parent` stand for, as PHP prints them.

## Why

A site's write rule held only the type's built-in bits, which have none for a class. So every object written to a class-, interface- or enum-typed property took the VM's general asynchronous property mutation: about 9,400 to 10,700 machine instructions against PHP's 98 to 111.
- **Now:** 398 to 432, the cost of an `int`-typed write.
- **Who pays for it:** Symfony services and Doctrine entities are written with exactly such types.

## Turned down

A bit in `TypeMask` per declared class: classes are not known when the unit is compiled, and a bit cannot express "extends or implements".
