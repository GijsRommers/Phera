# Trim the unused slot from a returning frame

- Date: 2026-11-23
- Area: Compiler and interpreter calls
- Decided by: Design

## Decision

Give returns a stack pop that releases the current frame and its side-table
entry. Unwinding and shutdown still need the popped frame. They copy it
before using the same pop operation.

After lowering, drop the trailing scratch slot when the register count proves
that it is still at the end and the instructions cannot use it. Keep it for
reference bindings and general comparison branches, including the VM's
comparison fallback. Keep the footprint for direct calls because even a
zero-argument call names the boundary of its argument area. Keep at least
one register in the window.

## Why

The instruction ledger counted 30 instructions around the frame pop and
seven per empty register scanned on return. Its predicted net saving was
12–24 instructions per function or method iteration. The shape and request
gates determine whether the change pays for itself in the interpreter.

## Structure: was, is

| Structure | Was | Is |
| --- | --- | --- |
| Return from PHP | Asked for a popped frame after reading its fields | Pops for side effects, with side-table cleanup on its cold path |
| Register window | Included a scratch slot even for a function that could never use it | Omits the unused trailing slot when its users are absent |
| Register release | Scanned every slot in the window | Scans a smaller window in the same order |

## Turned down

A per-frame bitmap would add work to register writes. The compiler can
identify the unused trailing slot once without changing the representation
of a value or the order in which live values are released.
