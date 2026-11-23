# `(array) $object` of an object with plain properties is built from its slots at its final size

- Date: 2026-11-23
- Area: Interpreter, values
- Decided by: Design

## Decision

`(array) $object` for an object whose class has no property hooks, no native property view and no lazy state does not go through the general property view. The cast reads the class's layout and the object's slots and pushes each entry straight into the array's storage, which is allocated once for the number of slots and undeclared properties.

**Where the keys come from.** The mangled keys (`\0Class\0name` for a private property, `\0*\0name` for a protected one, the plain name for a public one) are made once per class, when the class is linked (`PropertyLayout`). A cast clones each key's reference count; it formats nothing and looks nothing up. That was already so; what the cast did around it was not.

**What changed in the cast.**
- The result was built from an iterator that filters, which reports no size, so the entry vector grew from 0 to 4 to 8 to 16 slots: three allocations and two copies for an eight-property entity. It is one allocation now (`PhpArray::build_distinct`).
- The cast asked `Vm::for_object`, scanned every declared property for hooks and read the class's hook table on every call. A class says once, in its `PropertyFacts`, whether it has hooks; the cast reads that word.
- The cast copied the operand, opened an `OperatorDiagnostics` buffer (which an object cast never fills) and flushed it.

**Two results that differed from PHP are fixed with it** (the new fixture was made from PHP's output and failed on them):
- `(array)` of an object with an undeclared property whose name reads as an integer (`(object) [5 => 'five']`, `$o->{'5'}`) gives the integer key `5`, as PHP does. `get_object_vars` and `var_dump` keep the name as it is.
- `(array) $closure` is `[$closure]`; Phera gave `[]`.

## Why

Instructions per operation above an empty loop (`m.php` and `m5.php`, Linux, `perf stat -e instructions:u`, two iteration counts). Before: main `0c63b85f`. PHP 8.5.13 with OPcache on and JIT off (`php -n -d opcache.enable_cli=1 -d opcache.jit=disable`; it counts the same as without OPcache for these loops):

| operation | PHP 8.5 | before | after |
| --- | ---: | ---: | ---: |
| `$a = (array) $entity` (eight declared properties) | 673 | 2,364 | 1,688 |
| `((array) $entity)["\0Entity\0name"] ?? null` | 782 | 3,162 | 2,485 |
| accessor `getValue` (a method with that read) | 1,054 | 4,225 | 3,548 |
| `(array) $entity4` (4 properties) | 494 | 1,743 | 1,396 |
| `((array) $entity4)[$mangled] ?? null` | 592 | 2,507 | 2,187 |
| `(array) $entity40` (40 properties) | 2,206 | 10,963 | 8,596 |
| `((array) $entity40)[$mangled] ?? null` | 2,306 | 12,021 | 9,658 |

The cost grows with the number of properties, in PHP too (about 43 instructions a property) and faster in Phera (about 200 a property after this change, 250 before): each entry takes a reference count of its key and one of its value, and the array gives both back. The cast of 40 properties is 3.9 times PHP's.

Where the 1,688 are now (`perf record`, per cast, the loop's own 175 included):

| part | instructions |
| --- | ---: |
| the loop and the exit from it for an instruction the scalar tier does not take (`run`, `run_batch_loop`, `execute`, `set`) | about 570 |
| building the entries (`array_of`, `copy_element`) | about 420 |
| freeing the array: 8 keys and 8 values | about 300 |
| `execute_cast` | about 160 |

The first row is not this change's to make smaller. `Cast` to `bool` is a scalar-tier instruction in `fast.rs`; `Cast` to `array` is not, so every cast leaves the loop and comes back. Taking an object cast there is a change in `fast.rs`, which this work does not touch without asking. It would save about 350 instructions per cast.

## Reading one key of the cast and dropping it

Doctrine reads a field as `((array) $object)[$key] ?? null`. Whether the cast can be skipped for a key that is known:

**It can, for an object of a plain class, and a program cannot tell.**
- The array holds the property values, or a copy of each (a reference no one else owns is copied; any other stays a reference, which a read of the element gives through as its value). A read of one key gives that property's value, or nothing for a key the object does not have.
- With `??` nothing is reported for a missing key. Without it, PHP reports `Undefined array key "<key>"` with the key as the program wrote it; the fused form would have to give the same message.
- The array is not seen by anything between the cast and the read. It is dropped at the end of the statement, so no destructor runs earlier or later for having built it: the values it held are also held by the object.
- Memory use differs by the size of the array, for the length of the statement. `memory_limit` could only notice at the limit.

**It cannot for any other object.** A property with hooks gives its stored value in the array and not what `get` would; a lazy object that is not initialized gives no properties and is not initialized by the cast; `ArrayObject` and `ArrayIterator` give their storage; a closure gives itself; a class with native properties gives its view. These keep the cast.

So the rule is the one used above: the plain-class test (`PropertyFacts::HOOKS` clear, class not `Closure`, not lazy) is made at the read, and any object that fails it takes the cast and the lookup. The mangled key is a constant of the expression for a private or protected property, and `Doctrine` builds it per class.

It needs an instruction for "the property stored under this key, as `(array)` would hold it" and a compiler rule that finds `Cast(array)` followed at once by a read of an element with `??` or `isset` and no other use of the temporary. Both are changes to the instruction set and to the loop's dispatch, so they are not part of this pull request. Estimated cost for a read: a slot lookup by key in the layout (8 comparisons for an 8-property class, or a per-site slot hint as property sites have), about 100 to 150 instructions against 2,500 now.

## Structure: was, is

| structure | was | is |
| --- | --- | --- |
| entries of the cast array | pushed through a `Result` iterator that filters; the vector grows in steps | pushed into an array allocated for `slots + undeclared` (`PhpArray::build_distinct`) |
| is the object plain | `for_object` by identity, a scan of every property for hooks, the hook table | one read of `PropertyFacts` |
| diagnostics buffer, copy of the operand | taken and flushed on every cast | not made for an object |
| integer-like undeclared names | string keys | integer keys, in the cast only |
| `(array) $closure` | `[]` | `[$closure]` |

## Turned down

- **Making the keys once per class again.** They are made once per class already (`PropertyLayout`).
- **Skipping the scan of the array's values when dropping it.** It made an array of scalars cheaper (`bench-array` −0.3%) and an entity with one array property dearer (+74 per cast), because the scan runs twice for values that need the work list.
- **Building the array lazily.** Every use of an array value assumes it is complete.
