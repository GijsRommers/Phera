# A static property read by a named class is resolved once, and the loop then reads it from a site of three numbers

- Date: 2026-11-21
- Area: Properties
- Decided by: Design

## Decision

**The compiler.** `self::$p`, `static::$p` and `parent::$p` now compile to the same path base as `Post::$p`: a class symbol and a property name. Before, the class was a string made by `FetchClassName` and the name a constant, and the VM looked the class up by that string.

**The VM.** It resolves `self`, `static` and `parent` from the running frame, as `FetchClassName` does. One resolution gives the property's storage. A read that warns checks a typed static without a value on that storage; before, it looked the class and the property up a second time by name.

**The site.** After the VM has read a static property by a named class, with no steps or only index steps (`static::$resolved[$key]`), it records three numbers in the instruction's site:
- the class scope of the code (the class `self::` names, and the scope visibility was checked from);
- for `static::` only, the called class;
- the static's slot in the request's class states.

When the running code has the same scope (and called class), the interpreter loop reads the slot itself. It gives the value, the element, or, for `isset`, `??` and `empty`, null when a key is missing.

A site gets no entry, and the VM keeps the read, for:
- a static that is not visible from the code;
- a trait's own static (a deprecated access);
- a static whose default has not been evaluated yet;
- a typed static without a value.

In the loop, the VM keeps:
- a missing element in a plain read (a warning);
- a PHP reference on the way;
- a level that is not an array.

**The error message.** A private or protected static read from a scope that may not read it now names the class the access went through (`Child::$secret`), as PHP does. Before, it named the declaring class.

## Why

On the Laravel `/api/hello` route, `Fetch A::$p` runs 114 times per request at about 2,254 instructions each. That was the largest property item in the itemised request, above `$this->prop`.

Instructions per read:

| read | before | after | PHP |
| --- | ---: | ---: | ---: |
| `Base::$n` | 2,354 | 635 | 7 |
| `Child::$n` (inherited) | 2,385 | 632 | 12 |
| `self::$n` in a static method, with the call | 5,026 | 1,513 | 188 |
| `static::$instance`, with the call | 5,820 | 2,032 | 322 |
| `static::$resolved['a']`, with the call | 6,143 | 2,072 | 323 |
| `isset(static::$resolved['a'])`, with the call | 6,270 | 2,886 | 381 |

**Why a slot number is safe.** Slots are numbered by the worker. An entry made before the snapshot therefore names the same static in every forked request, and it holds no pointer into one request's storage.

**Why the guard is enough.** For a named class and `self::`, the class scope fixes the class and the visibility. For `parent::` it fixes the parent too. For `static::` the called class is checked as well. Two integer comparisons decide a hit.

## Turned down

- **Holding the storage `Rc` in the site:** a forked request has its own storage, so the site would read the snapshot's value.
- **A new path base for `self`, `static` and `parent`:** the class symbol already carries those names, and `FetchClassName` resolves them, so no bytecode change was needed.

## Not in this change

Writes to statics (`static::$n = $v`, about 4,800) and `++` on them keep the VM. They come next, with the readonly and error paths.
