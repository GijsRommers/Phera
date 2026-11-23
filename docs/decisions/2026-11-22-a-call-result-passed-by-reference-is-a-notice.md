# A call result passed to a by-reference parameter is a notice, not an error

- Date: 2026-11-22
- Area: Scope and compatibility
- Decided by: Fable

## Decision

```php
echo end(explode(',', 'a,b,c'));   // PHP: Notice: Only variables should be passed by reference, then "c"
echo end($a . $b);                 // PHP: Error: end(): Argument #1 ($array) could not be passed by reference
```

Phera threw the error for both. The first form is everywhere in older PHP code
(`end(explode(...))`, `array_pop(f())`, `reset(array_filter(...))`), so it was a fatal error where
PHP runs the program.

PHP's rule is about how the argument is written, not about its value: a function call, a method
call (also `?->`), a static call, a call of a closure or of an invokable object, `new`, `include`
and `eval` pass with the notice; every other computed value (a ternary, `??`, a cast, a
concatenation, a literal, an assignment, `match`, a constant, `clone`, `strlen(...)` as a closure)
is the error.

One more thing decides it, and a php-src test depends on it (`Zend/tests/bug79783.phpt`,
`str_replace("a", "b", "c", strlen("d"))` must be the error): a call that PHP 8.5 does not compile
as a call is not "the result of a call". That is the case for the functions its compiler turns
into an operation of their own (`strlen`, `count`, the `is_*` tests, `intval` and the other casts,
`gettype`, `get_class`, `func_get_args`, `array_key_exists`, a literal `defined`, `chr`, `ord`, a
`sprintf` with only `%s` and `%d`) and for those it calls without a frame (`in_array`, `trim`,
`implode`, `min`, `max`, `substr`, `strpos`, `str_replace` with three arguments, and a dozen
more), each for certain argument counts only, and only where the name is certain to be the global
function: `namespace App; strlen($s)` may be `App\strlen`, so there it is a call again. A program
sees the difference, so `crates/compiler/src/expressions/call_results.rs` holds the two lists as
PHP 8.5 has them. They are data about PHP's compiler, not about ours: Phera compiles some of these
calls differently and still answers as PHP does. When PHP changes a list, this file changes.

So the compiler decides it:

- `Instruction::SendValue` and `Instruction::SendTake` carry `of_call`: the argument was written as
  a call or as `new`.
- The VM's general send (`Vm::send`, the only place a by-reference parameter is bound) turns such a
  value into `SentArgument::Result`. For a parameter that must be a reference it raises the notice
  and binds a new reference holding the value. If the register holds a reference, the function
  returned one (`function &f()`): the parameter binds to it and there is no notice, as in PHP.
- A handler that throws on notices (Laravel's does) stops the call before it is made, as in PHP.
- The takes pass leaves a send that says `of_call` as a send of a register: the literal form
  (`SendConst`) has no place for the flag, and a call folded to a literal is rare.

The same change repairs a second difference. A call written straight into the argument area of a
direct call left a returned reference there, and the callee's by-value parameter then was that
reference: `grow(shared())` changed the variable `shared()` returned. `Vm::call_direct_general`
now reads such a reference; every parameter of a direct call is by value (the compiler makes no
direct call to a function with a by-reference parameter).

## Why this shape

The interpreter loop does not look at the new field: its arms match `SendValue { src, .. }`, and a
by-reference parameter always left the loop for `Vm::send`. The field fits in the instruction's
unused bytes (the size assertion in `instruction.rs` holds), so the cost per send is nothing; the
request gates confirm it (see the pull request).

## Turned down

- **Working it out at run time from the instruction before the send** (is it a `Call` that wrote
  this register, and does no jump land in between): needs the list of every jumping instruction in
  the VM, and gets `?->` wrong, whose result arrives at a join like a ternary's.
- **A separate instruction (`SendResult`)**: the takes pass would need a taking form of it too, and
  the loop two more arms.
- **A side table of send positions per function**: every pass that renumbers instructions would
  have to keep it in step.

## Structure: was, is

| structure | was | is |
| --- | --- | --- |
| `SendValue`, `SendTake` | `{ src }` | `{ src, of_call }` |
| the compiler's `Argument` | source, name, spread | and `of_call`, set from the syntax tree |
| `SentArgument` | `Value`, `Literal`, `Local`, `Path` | and `Result` |

## Tests

- `tests/fixtures/calls/call-result-to-a-by-reference-parameter.phpt`: every argument shape above
  against user functions, methods, static methods, a constructor and native functions; the
  notice's place relative to the argument's own side effects; a returned reference bound and
  read; a handler that throws.
- `tests/fixtures/calls/call-result-compiled-away.phpt`: 122 calls, every name of the two lists
  with the argument counts on both sides of its rule.
- `tests/fixtures/calls/call-result-in-a-namespace.phpt`: unqualified, imported, fully qualified
  and namespace-relative names.

Expected output from PHP 8.5 in all three.
