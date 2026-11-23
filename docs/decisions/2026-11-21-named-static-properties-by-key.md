# A static property named in the source finds its class by key

- Date: 2026-11-21
- Area: Engine, classes
- Decided by: Claude (design), approved by Fable (second form, only the named case)

## Decision

- `Post::$count`, where the source names both the class and the property, compiles to the path base `NamedStaticProperty { class: SymbolId, name: NameId }`. `self::`, `parent::`, `static::`, `$class::$x` and `Post::$$name` keep `StaticProperty`, whose class and name are registers.
- A read of the named form takes a ready path: the class is found by the symbol's process name key, it is not a trait, the property is declared and public, and its storage exists and is initialized. Then the value is returned with no string built and no name hashed.
- Everything else (a protected or private property, a class not declared yet, which autoloads, a trait, uninitialized storage, any write, `++`, a reference) goes to the general handler. When the class was found, the class and property name are handed on so the lookup does not run twice.
- One helper (`static_places::static_base`) gives every place handler the class and name of either form, so the handlers share one code path.

## Why

The design rules ask for names that resolve to indexes when the code is linked, not names hashed at run time. 2.5% of a forked `/api/posts` request ran under `static_property`, building the class's lowercase name and looking it up by string.

## Turned down

- A flag on `StaticProperty` instead of a second form. The existing handlers keep their code, and the hot loop does not branch on the flag.
- A string-keyed cache of resolved static properties.
