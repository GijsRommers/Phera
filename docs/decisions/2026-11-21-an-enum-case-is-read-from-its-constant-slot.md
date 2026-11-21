# An enum case is read from its constant slot

- Date: 2026-11-21
- Area: Engine, classes
- Decided by: Fable (found by measuring single operations against PHP)

## Decision

- Every case of an enum written in PHP gets a constant slot when the enum is linked, the same per-request slot an evaluated class constant has (`ClassStates`). The case constant carries the slot number.
- When a case object may be handed out without any further work, the request stores it in that slot. From then on `Suit::Hearts` is read like `Post::LIMIT`: by the ready read of a class constant, inside the interpreter loop.
- A unit enum's case is stored when it is made. A backed enum's cases are stored together, when all of them exist and their backing values were found to be distinct. Until then every read goes through the case lookup, which starts that check, so an enum whose backing values turn out equal reports it on every use, as before.
- A case marked `#[Deprecated]` keeps the general path, which reports it on each read.
- `from()` and `tryFrom()` start the check of the backing values only while it has not been done. `cases()` takes stored cases directly.
- An ordinary constant of a unit enum (`const FIRST = self::X`) is read in the loop like a class's. A backed enum's ordinary constants stay out of line, because the loop cannot see whether the cases were checked.
- The cases of a native unit enum (declared by an extension) have no slot and are fetched as before.

## Why

Measured in a loop inside a function, instructions per iteration above an empty loop, main `8732c15a` and PHP 8.5.12:

| Operation | PHP | Before | After |
|---|---:|---:|---:|
| `S::A === $e ? 1 : 0` | 86 | 4,071 | 1,113 |
| `match ($e) { S::A => 1, S::B => 2, S::C => 3 }` | 192 | 7,811 | 1,892 |
| `U::X === U::Y ? 1 : 0` (unit enum) | 121 | 6,661 | 1,582 |
| `S::from($value)` | 500 | 9,587 | 6,361 |
| `S::cases()` | 591 | 7,119 | 5,214 |
| `Holder::KIND === $e ? 1 : 0` (a class constant holding a case) | 88 | 1,104 | 1,111 |

A case constant was never "ready": each fetch left the loop, copied the class and case names into new strings, and ran the initializer machinery to find an object that already existed. A request that hydrates entities with enum columns and compares them against cases does this thousands of times. After the change a case fetch costs what a fetch of a class constant holding the same object costs. What is left in that row is the comparison and the ternary, and in `from()` and `cases()` the price of calling a native method from PHP code, which is a separate item.

The structure follows the design rules: the case object lives in a flat table indexed by a number fixed at link time, next to the other evaluated constants, instead of being found through two hash tables keyed by class and by name on every read. Those tables remain as the place where cases are made.

## Turned down

- Looking the case up in the existing tables from the ready read. That would still hash the case name on every fetch.
- A second table of finished cases per enum. The constant slots already are that table, and they are already copied when a request is forked from the snapshot, with object identity kept.
- Storing a backed enum's case as soon as it is made. Its backing values are checked on the first use of the enum; a stored case would let later reads skip a check that failed.
