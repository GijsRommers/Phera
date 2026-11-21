# A method call with locals and literals as arguments is one instruction

- Date: 2026-11-21
- Area: Compiler, engine (calls)
- Decided by: Claude (design), approved by Fable

## Decision

- `$order->total($tax, 2)` compiles to one `CallMethod { site, dst, result }` when every argument is a local or a literal. The function's `method_sites` table holds the receiver register, the method name and the run of `site_arguments` (a local, which a by-reference parameter would bind to, or a temporary holding a literal).
- The call written out the general way (`InitMethodCall`, one `Send` per argument, `Call`, then a jump back after `CallMethod`) follows the function's code. The loop's quick entry takes a method written in PHP that the receiver's class has and the caller's scope may call, plain in shape, with arguments that are defined, not references, and pass their type tests. Everything else jumps to that general sequence, which the VM runs with every rule.
- Calls inside a `try` keep the general sequence: the fallback lies outside the protected range.
- The pass runs after every other compiler pass, so the optimizing passes never see `CallMethod`. Where the optimizer reused one temporary for two literals, the second load gets a register of its own, because the site reads all arguments at the call.

## Why nothing observable happens between the lookup and the arguments

PHP finds the method (`INIT_METHOD_CALL`) before it evaluates the arguments: an undefined method throws before an argument with a side effect runs. Reading a local or loading a literal has no effect a program can see. A literal load cannot fail, and a local is only read. So in these calls, finding the method before or after the arguments makes no difference. A local that is undefined, or a reference, makes the quick entry decline, and the general sequence then warns after the lookup, as PHP does. An argument computed any other way (a call, an operator, a property read, a named or spread argument) keeps the general sequence, and the fixtures in `crates/runtime/tests/method_calls.rs` cover each case where the order is observable.

## Turned down

- Replaying `InitMethodCall`, the sends and `Call` inside one VM handler: a handler that needs the async shell midway (autoload, `__call` running PHP code) could not resume the rest. The out-of-line general sequence keeps one definition of the behaviour.
- A per-site memo of the resolved method: not built in this step (see the pull request for what the lookup costs).
