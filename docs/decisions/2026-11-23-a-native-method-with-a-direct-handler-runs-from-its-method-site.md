# A native method with a direct handler and arguments runs from its method site, and a reflected property answers only for objects of its class

- Date: 2026-11-23
- Area: Interpreter, Reflection
- Decided by: Design

## Decision

`$property->getValue($object)`, `$property->setValue($object, $value)`, `setRawValue`, `setRawValueWithoutLazyInitialization` and `isInitialized` call a method that has a direct handler (`MethodEntry::direct`). A method site that finds such a method for its receiver's class records the handler and what each of the site's arguments may be for the handler to take it as it is (`SiteOutcome::Direct`). The next call of the site runs the handler on the arguments in their registers and stores its value where the call's result goes. It makes no prepared call, no call record and no copy of the method's description.

**Which calls.** A public native method, not static, with a direct handler and no preparation of its own, no by-reference and no variadic parameter, called with at least as many arguments as it requires and at most as many as it has, and at most three. Any other native method keeps the general sequence (`InitMethodCall`, the sends, `Call`).

**What the call checks each time.**
- The receiver is not a lazy object.
- Every argument is defined, is not a PHP reference and has a type its parameter takes as it is (the same test `accepts_direct` made on every call, made once per site for everything except the argument's own type).
- The handler does not decline.

A call that fails any of these leaves everything as it was, and the general sequence that follows the site's `CallMethod` runs it, with PHP's errors, conversions, hooks and lazy initialization.

**How the handler gets the arguments.** One argument is borrowed from its register. Several are taken out of their registers for the call and put back after it: no value is copied or dropped. A variable passed twice (`set($o, $o)`) declines, because the second use finds its register empty.

**What the site record costs.** A record is copied out of its cell at every method call, so `SiteOutcome` stays at 16 bytes, and a compile-time assertion keeps it there. The first version made it 24 bytes and made `bench-hierarchy` 1.5% and `bench-objects` 1.3% dearer in instructions without any of them calling a native method.

**A reflected property answers only for objects of its class.** `ReflectionProperty::getValue($object)` for an object that is not an instance of the property's declaring class throws `ReflectionException` in PHP. The direct handlers decided by the key in the slot, so two objects answered where PHP throws:
- an object of the parent of a class that redeclares the property (`Redeclared::$name` read from a `Product`, which has `name` in the same slot);
- an object of an unrelated class that declares a property of the same name in the same slot.

A reflector now keeps the layout of the class it was made for (`SlotAccess::layout`). An object with that layout is of the class, by one pointer comparison, and skips the key comparison too. Any other object is asked `is_instance_of` the declaring class. This is the case a program that reflects a parent's property and reads a subclass's object pays for.

## Why

Instructions per operation above an empty loop, `m.php` of this pull request (Linux, `perf stat -e instructions:u`, two iteration counts; before: main `0c63b85f`; PHP 8.5.13 with OPcache on and JIT off, `php -n -d opcache.enable_cli=1 -d opcache.jit=disable`, which counts the same as without OPcache for these loops):

| operation | PHP 8.5 | before | after |
| --- | ---: | ---: | ---: |
| `ReflectionProperty::getValue($e)` | 206 | 1,554 | 522 |
| `ReflectionProperty::setValue($e, 'n')` | 306 | 1,919 | 788 |
| `ReflectionProperty::setRawValueWithoutLazyInitialization($e, 'n')` | 376 | 1,912 | 788 |
| accessor `setValue` (a method that tests `instanceof` and calls the line above) | 670 | 3,923 | 2,744 |
| hydrate one row of five fields | 6,626 | 42,823 | 36,935 |

The calls are still two and a half times PHP's. What is left is the cost of any native method call from a site, not of reflection: a method with no argument and nothing to do (`$method->isStatic()`) costs 377 here against 132 in PHP. That cost is in `call_method` and the loop that dispatches it (about 300 of the 522), which this change leaves to the work on method calls.

`bench-spl` is 15% lower in instructions, `bench-natives` 2.6% and `bench-reflection` 2.3%, because every native method with a direct handler and arguments takes this path: `ArrayObject`, `SplFixedArray`, `SplObjectStorage`, `DateTime` and the others.

## Structure: was, is

| structure | was | is |
| --- | --- | --- |
| a call of a native method with a direct handler and arguments | a prepared call with the method's name, parameters, method and context cloned, the sends, `Call`, `finish_direct_method` | one `CallMethod` that runs the handler |
| what a site records for it | `SiteOutcome::Native` | `SiteOutcome::Direct(handler, masks)` |
| whether an argument fits | `accepts_direct` computes each parameter's mask at every call | the masks are made once, when the site resolves |
| object check of `ReflectionProperty` direct handlers | the key in the slot | the layout of the reflector's class, else `is_instance_of` the declaring class |

## Turned down

- **A direct handler that takes the VM's registers.** The handlers are in extensions that do not see the VM. They keep `fn(&Object, &[Option<Value>])`.
- **Copying the arguments into an array on the stack.** One copy of an object and a string, and their drop, cost 80 instructions more than lending them. For one argument no lending is needed: the handler gets the register's own slot.
- **Making `Direct` carry the whole `MethodEntry`.** It is 80 bytes, and the record would then be 100 bytes.
- **Declining every redeclared property.** The class check costs one pointer comparison for the common case.

## Not built

The method call itself (`call_method`, the dispatch of `CallMethod`) is not changed, and neither are `execution_loop.rs` and `fast.rs`.
