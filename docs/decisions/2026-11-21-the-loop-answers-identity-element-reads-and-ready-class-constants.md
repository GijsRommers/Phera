# The interpreter loop answers identity, reads of local array elements, undeclared `instanceof` and ready class constants

- Date: 2026-11-21
- Area: Interpreter
- Decided by: Design

## Decision

Four shapes that left the loop for `Vm::execute` now run inside it:
- `===` and `!==` take operands of every kind. Identity compares objects by identity and arrays element by element, and runs no PHP code; a diagnostic or an error still sends the instruction to the VM.
- A read, `isset`, `??`, `empty` or destructuring of a local array element walks up to three integer or string keys through arrays (`calls/fast_local_elements.rs`). These go to the VM:
  - a missing element in a plain read (it warns);
  - a level that is not an array;
  - a reference root;
  - a function with a dynamic scope.
- `instanceof` of a name that no class, interface or alias of the request has answers `false`, as PHP does without autoloading.
- `FetchNamedClassConstant` reads a public, non-deprecated constant of a class that is not an enum, through borrows, when its value is ready. The loop borrows the request's class states for it. The first read, protected and private constants, enum cases, deprecations and undeclared classes stay with the VM's handler.

## Why

On the Symfony and Doctrine yardstick these were 5,515 of 39,248 loop exits per request, at 1,000 to 1,300 machine instructions each. `isset($row['k'])` on a local array cost 1,220 against PHP's 115.
