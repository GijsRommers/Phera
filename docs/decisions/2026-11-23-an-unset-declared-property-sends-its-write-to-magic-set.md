# An unset declared property sends its next write to `__set`, also from the fast write path

- Date: 2026-11-23
- Area: Scope and compatibility
- Decided by: Design

## Decision

```php
class C { public $p = 1; function __set($n, $v) { echo "__set $n\n"; } }
$c = new C;
unset($c->p);
$c->p = 5;          // PHP: "__set p"; the property stays unset
```

The property table of #220 lets the interpreter loop write a plain declared property straight into
its slot. It also filled a slot that held no value, so after `unset()` the write went into the object
instead of to `__set`, and the next read found the value instead of calling `__get`. Three php-src
tests failed on it (`Zend/tests/magic_methods/bug63462.phpt`,
`Zend/tests/type_declarations/typed_properties_magic_set.phpt`, `tests/classes/unset_properties.phpt`).

Now the table records whether the class has `__set` (`PropertyTable::fills_missing_directly`). With
`__set`, a write that finds the slot empty leaves the loop for the general path, which knows an
`unset()` property (to `__set`) from a typed property that was never initialized (filled). Without
`__set`, nothing changes: an empty slot is filled in the loop as before.

## Why this shape

The slot alone cannot tell "unset" from "never initialized"; the general path can. A class with
`__set` is rare among the classes whose properties are written in hot code, so sending only its
empty-slot writes to the general path costs nothing measurable (see the pull request).

## Turned down

- Sending every empty-slot write to the general path: constructors assigning typed properties are
  exactly that, in every Doctrine entity.

## Structure: was, is

| structure | was | is |
| --- | --- | --- |
| `PropertyTable` flags | native properties, `__get`, `__isset`, statics, hashed | and `__set` |
| `write_slot_direct` | fills an empty slot | fills it only when the class has no `__set` |

## Tests

`tests/fixtures/classes/unset-declared-property-goes-to-magic-set.phpt`: unset public, typed,
protected and private properties written and read from outside and inside the class, a typed
property never initialized (filled, no `__set`), a class without `__set`, a warm loop. Expected
output from PHP 8.5.
