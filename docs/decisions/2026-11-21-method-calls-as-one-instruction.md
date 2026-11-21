# A method call with locals and literals as arguments is one instruction

- Date: 2026-11-21
- Area: Compiler, engine (calls)
- Decided by: Claude (design), approved by Fable

## Decision

- `$order->total($tax, 2)` gets one `CallMethod { site, dst, result }` in front of its `InitMethodCall`, `Send`s and `Call` when every argument is a local or a literal. The function's `method_sites` table holds the receiver register, the method name, the run of `site_arguments` (a local, which a by-reference parameter would bind to, or a temporary holding a literal) and `after`, the instruction after the `Call`.
- `CallMethod` enters a method written in PHP that the receiver's class has and the caller's scope may call, plain in shape, with arguments that are defined, not references, by value and accepted by their types; the caller goes on at `after`. For anything else the loop simply goes on with the next instruction, the general sequence, which applies every rule: a native method, `__call`, an error, a closure or a generator; a PHP method whose arguments need the general rules gets the prepared call `InitMethodCall` would make, and its sends run.
- The literal loads of such a call come before `CallMethod`; a literal loaded into a register the call loaded before gets a register of its own. Jumps to the call land on its start, and `try` ranges and `finally` targets are renumbered, so calls inside a `try` get a `CallMethod` too.
- Each site keeps a record per VM (one fixed entry, made when its function first runs a site): the receiver's class index and what the site does for it (enter this PHP method, leave a native method to the general sequence, leave it to the VM). A receiver of another class looks the method up and replaces the record; closures keep none, since their scope can change.
- The pass runs after every other compiler pass, so the optimizing passes never see `CallMethod`.

## Why nothing observable happens between the lookup and the arguments

PHP finds the method (`INIT_METHOD_CALL`) before it evaluates the arguments: an undefined method throws before an argument with a side effect runs. Reading a local or loading a literal has no effect a program can see. A literal load cannot fail, and a local is only read. So in these calls, finding the method before or after the arguments makes no difference. A local that is undefined, or a reference, makes the quick entry decline, and the general sequence then warns after the lookup, as PHP does. An argument computed any other way (a call, an operator, a property read, a named or spread argument) keeps the general sequence, and the fixtures in `crates/runtime/tests/method_calls.rs` cover each case where the order is observable.

## Turned down

- Replacing the three instructions by one and keeping the general sequence out of line after the function's code (the first version, #165): every call the quick entry did not make paid a jump there and back, 250 to 400 instructions per native method call.
- Replaying `InitMethodCall`, the sends and `Call` inside one VM handler: a handler that needs the async shell midway (autoload, `__call` running PHP code) could not resume the rest.
- A hash table or a growing list of classes per site: the record is one fixed entry, replaced when the class changes.
