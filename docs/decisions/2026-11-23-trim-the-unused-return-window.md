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

## Measurements

Normal release binaries on Apple silicon, compared with main at `0c63b85f`.
The function and method in the four-shape study both use two registers
where they used to use three. In the generated machine code the common
return no longer calls the general frame pop. The cold side-table cleanup
still has a call site.

| Shape | Main instructions per iteration | With this change | Difference |
| --- | ---: | ---: | ---: |
| Loop | 145.196 | 145.207 | +0.011 |
| Function | 679.414 | 651.419 | −27.995 |
| Method | 857.409 | 829.414 | −27.995 |
| Property | 549.210 | 549.215 | +0.005 |

| Route | Main, millions of instructions per request | With this change | Difference | Repeat |
| --- | ---: | ---: | ---: | ---: |
| Laravel hello | 23.143 | 23.054 | −0.38% | −0.34% |
| Laravel posts | 104.480 | 104.215 | −0.25% | −0.26% |
| Symfony database yardstick | 432.71 | 431.71 | −0.23% | |

Laravel used six alternating pairs per gate. The spread warranted a second
gate on both Laravel routes. Symfony used two pairs with 300 warm-up requests
per worker. The ten benchmarks used five paired runs each. Every minimum and
median fell, by 0.55% to 3.33%.
